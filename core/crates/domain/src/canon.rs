//! Text canonicalization.
//!
//! Contract (`docs/contracts/working-db.sql` header, README Part 3):
//! every hash is computed over NFC-normalized, trimmed text with internal
//! whitespace collapsed and **case preserved** (TTS is case sensitive).
//!
//! Canonicalization is idempotent: `canonicalize(canonicalize(x)) == canonicalize(x)`.
//! That property is what lets morphod store the canonical form of candidate
//! text and still reproduce the same hash later.

use unicode_normalization::UnicodeNormalization;

/// Normalize to NFC, trim outer whitespace, collapse internal whitespace runs
/// to a single ASCII space. Case is preserved.
///
/// "Whitespace" is the Unicode `White_Space` property (so NBSP, tabs, newlines
/// and ideographic space all collapse to a plain space).
pub fn canonicalize(input: &str) -> String {
    let normalized = input.nfc();
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for ch in normalized {
        if ch.is_whitespace() {
            // Leading whitespace is dropped because nothing has been written yet.
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(ch);
        }
    }
    out
}

/// Canonical form used as a lookup key for lexicon matching (`words.lemma` is
/// `COLLATE NOCASE`, and the lemmatizer emits lowercase). Case folded on top of
/// [`canonicalize`].
pub fn fold_lemma(input: &str) -> String {
    canonicalize(input).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_outer_whitespace() {
        assert_eq!(canonicalize("  hello  "), "hello");
        assert_eq!(canonicalize("\n\thello\r\n"), "hello");
    }

    #[test]
    fn collapses_internal_whitespace() {
        assert_eq!(canonicalize("well   meaning"), "well meaning");
        assert_eq!(canonicalize("well\t\nmeaning"), "well meaning");
        assert_eq!(canonicalize("a  b   c"), "a b c");
    }

    #[test]
    fn collapses_unicode_whitespace() {
        // U+00A0 NO-BREAK SPACE, U+2003 EM SPACE, U+3000 IDEOGRAPHIC SPACE.
        assert_eq!(canonicalize("well\u{00a0}meaning"), "well meaning");
        assert_eq!(canonicalize("well\u{2003}\u{2003}meaning"), "well meaning");
        assert_eq!(canonicalize("\u{3000}well meaning\u{3000}"), "well meaning");
    }

    #[test]
    fn preserves_case() {
        assert_eq!(canonicalize("  Well Meaning  "), "Well Meaning");
        assert_ne!(canonicalize("abandon"), canonicalize("Abandon"));
    }

    #[test]
    fn applies_nfc_composition() {
        // "e" + COMBINING ACUTE ACCENT must compose to U+00E9.
        let decomposed = "cafe\u{0301}";
        let composed = "caf\u{00e9}";
        assert_eq!(canonicalize(decomposed), composed);
        assert_eq!(canonicalize(decomposed), canonicalize(composed));
        assert_eq!(canonicalize(decomposed).chars().count(), 4);
    }

    #[test]
    fn is_idempotent() {
        let samples = [
            "  well   meaning\tand kindly ",
            "cafe\u{0301}",
            "",
            "   ",
            "single",
            "Ünïcödé  mix\u{00a0}ed",
        ];
        for s in samples {
            let once = canonicalize(s);
            assert_eq!(canonicalize(&once), once, "not idempotent for {s:?}");
        }
    }

    #[test]
    fn empty_and_whitespace_only_collapse_to_empty() {
        assert_eq!(canonicalize(""), "");
        assert_eq!(canonicalize("   \t\n "), "");
        assert_eq!(canonicalize("\u{00a0}"), "");
    }

    #[test]
    fn does_not_strip_inner_punctuation() {
        assert_eq!(canonicalize(" a  (b),  c! "), "a (b), c!");
    }

    #[test]
    fn fold_lemma_lowercases() {
        assert_eq!(fold_lemma("  Kindly  "), "kindly");
        assert_eq!(fold_lemma("KIND"), fold_lemma("kind"));
    }
}
