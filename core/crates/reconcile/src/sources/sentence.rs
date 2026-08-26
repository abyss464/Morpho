//! Turning a raw sentence from any source into an example candidate.
//!
//! Every example source — the exam corpus, the Free Dictionary payload,
//! Tatoeba — needs the same three things, and getting any of them subtly
//! different per source is how a highlight ends up pointing at the wrong bytes:
//!
//! * **canonicalize first.** `example_candidates.hl_start` is a UTF-8 byte
//!   offset into the *canonicalized* text (working-db.sql), and canonicalization
//!   collapses internal whitespace, so an offset taken from the raw string is
//!   simply wrong.
//! * **locate the target word.** Matching is case-insensitive, whole-word, and
//!   accepts the regular English inflections, so "adapt" highlights `adapt` in
//!   "Species adapt…" and `adapted` in "Species adapted…".
//! * **drop what cannot be located.** A sentence that does not contain the word
//!   it claims to illustrate is unusable for the mode-1 card. A wrong highlight
//!   is worse than a missing example, so it is never guessed.

use morpho_domain::canon::{canonicalize, fold_lemma};
use morpho_domain::types::FetchedExample;

/// Shortest sentence worth storing, in bytes. Below this it is a fragment, not
/// a sentence that shows the word doing anything.
const MIN_BYTES: usize = 12;
/// Longest sentence worth storing. The scorer's length window already tails off
/// well before here; this only keeps a runaway paragraph out of the library.
const MAX_BYTES: usize = 320;

/// Regular English inflections, longest first so `-ies` wins over `-s`.
///
/// This is the "simple inflection" set: `s`/`es`/`ed`/`d`/`ing` and the
/// spelling variants that go with them.
const SUFFIXES: &[&str] = &["ies", "ing", "ied", "ees", "es", "ed", "en", "er", "s", "d"];

/// Build one example candidate from a raw sentence, or `None` if it is
/// unusable.
pub fn candidate(raw: &str, lemma: &str, source_ref: Option<String>) -> Option<FetchedExample> {
    let text = canonicalize(raw);
    if text.len() < MIN_BYTES || text.len() > MAX_BYTES {
        return None;
    }
    let (hl_start, hl_end) = locate(&text, &fold_lemma(lemma))?;
    Some(FetchedExample {
        text,
        hl_start: hl_start as i64,
        hl_end: hl_end as i64,
        source_ref,
    })
}

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

/// Does `text` contain `lemma` or a simple inflection of it?
pub fn mentions(text: &str, lemma: &str) -> bool {
    locate(&canonicalize(text), &fold_lemma(lemma)).is_some()
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

    fn span(example: &FetchedExample) -> &str {
        &example.text[example.hl_start as usize..example.hl_end as usize]
    }

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
    fn matching_ignores_capitalization() {
        let example = candidate("Benevolent donors funded it.", "benevolent", None).unwrap();
        assert_eq!(span(&example), "Benevolent");
        let example = candidate("A BENEVOLENT gesture indeed.", "BeNeVoLeNt", None).unwrap();
        assert_eq!(span(&example), "BENEVOLENT");
    }

    #[test]
    fn a_sentence_without_the_word_is_rejected() {
        assert!(candidate("This never mentions it at all.", "benevolent", None).is_none());
        assert!(!mentions("This never mentions it at all.", "benevolent"));
        assert!(mentions("Chinese dragons are benevolent.", "benevolent"));
    }

    #[test]
    fn offsets_are_taken_after_canonicalization() {
        let example = candidate("The  benevolent   donor  funded it.", "benevolent", None).unwrap();
        assert_eq!(example.text, "The benevolent donor funded it.");
        assert_eq!(span(&example), "benevolent");
        assert_eq!(example.hl_start, 4);
    }

    #[test]
    fn offsets_land_on_character_boundaries_with_accents() {
        let example = candidate("A café by a serene lake side.", "serene", None).unwrap();
        assert!(example.text.is_char_boundary(example.hl_start as usize));
        assert!(example.text.is_char_boundary(example.hl_end as usize));
        assert_eq!(span(&example), "serene");
    }

    #[test]
    fn fragments_and_paragraphs_are_rejected() {
        assert!(candidate("adapt", "adapt", None).is_none());
        let long = format!(
            "The word adapt appears here. {}",
            "filler words. ".repeat(40)
        );
        assert!(candidate(&long, "adapt", None).is_none());
    }

    #[test]
    fn provenance_rides_along() {
        let example = candidate(
            "A serene lake lay below.",
            "serene",
            Some("tatoeba:42".into()),
        )
        .unwrap();
        assert_eq!(example.source_ref.as_deref(), Some("tatoeba:42"));
    }

    #[test]
    fn an_empty_lemma_never_matches() {
        assert_eq!(locate("anything at all here", ""), None);
    }
}
