//! Finding a word inside a sentence.
//!
//! `example_candidates.hl_start` / `hl_end` are UTF-8 byte offsets into the
//! **canonicalized** text (working-db.sql), which is what makes locating the
//! word a shared rule rather than a per-source detail: canonicalization
//! collapses internal whitespace, so an offset measured against the raw string
//! points at the wrong bytes, and every path that stores a sentence has to
//! measure against the same string the row will hold.
//!
//! This lives beside [`canonicalize`](crate::canon::canonicalize) for that
//! reason — the harvesters that build a candidate and the store that writes one
//! must agree, and the store cannot depend on the harvesters.

/// Regular English inflections, longest first so `-ies` wins over `-s`.
///
/// This is the "simple inflection" set: `s`/`es`/`ed`/`d`/`ing` and the
/// spelling variants that go with them.
const SUFFIXES: &[&str] = &["ies", "ing", "ied", "ees", "es", "ed", "en", "er", "s", "d"];

/// Byte range of `lemma` inside the already-canonicalized `text`.
///
/// Prefers the exact form before falling back to an inflection: a sentence
/// containing both "adapted" and "adapt" highlights the bare form.
pub fn locate(text: &str, lemma: &str) -> Option<(usize, usize)> {
    let lower = text.to_lowercase();
    let needle = lemma.to_lowercase();
    if needle.is_empty() {
        return None;
    }

    if let Some(range) = find_word(&lower, &needle) {
        return Some(range);
    }
    for suffix in SUFFIXES {
        let candidate = format!("{needle}{suffix}");
        if let Some(range) = find_word(&lower, &candidate) {
            return Some(range);
        }
        // Stem changes: "adapt" → "adapting", "serene" → "serener".
        if let Some(stem) = needle.strip_suffix('e') {
            if let Some(range) = find_word(&lower, &format!("{stem}{suffix}")) {
                return Some(range);
            }
        }
    }
    None
}

/// Find `needle` in `haystack` on word boundaries. Both must be lowercase.
///
/// Offsets are byte offsets, and because `to_lowercase` can change byte length
/// for some scripts, a mismatch between the folded and original lengths makes
/// this bail out rather than return a range that would slice mid-character.
fn find_word(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';
    let mut from = 0usize;
    while let Some(found) = haystack[from..].find(needle) {
        let start = from + found;
        let end = start + needle.len();
        let before_ok = haystack[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word(c));
        let after_ok = haystack[end..].chars().next().is_none_or(|c| !is_word(c));
        if before_ok && after_ok {
            return Some((start, end));
        }
        from = start + needle.chars().next().map_or(1, char::len_utf8);
        if from >= haystack.len() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_on_word_boundaries() {
        // "ample" must not match inside "example".
        assert_eq!(locate("An example of this.", "ample"), None);
        assert_eq!(locate("An ample supply.", "ample"), Some((3, 8)));
    }

    #[test]
    fn prefers_the_exact_form_over_an_inflection() {
        let text = "Species adapted, and species adapt.";
        let (start, end) = locate(text, "adapt").unwrap();
        assert_eq!(&text[start..end], "adapt");
    }

    #[test]
    fn accepts_the_simple_inflections() {
        for (text, lemma, expected) in [
            ("She adapts quickly.", "adapt", "adapts"),
            ("He watches closely.", "watch", "watches"),
            ("They adapted fast.", "adapt", "adapted"),
            ("The lake serened over.", "serene", "serened"),
            ("Species are adapting fast.", "adapt", "adapting"),
            ("The lake is serener today.", "serene", "serener"),
        ] {
            let (start, end) = locate(text, lemma).unwrap_or_else(|| panic!("{text} / {lemma}"));
            assert_eq!(&text[start..end], expected);
        }
    }

    #[test]
    fn an_empty_lemma_never_matches() {
        assert_eq!(locate("anything at all here", ""), None);
    }
}
