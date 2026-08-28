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

/// The matching rule itself lives in `morpho_domain`, because the store writes
/// example rows too and cannot depend on this crate. Re-exported here so every
/// caller keeps the one import it always had.
pub use morpho_domain::sentence::locate;

/// Shortest sentence worth storing, in bytes. Below this it is a fragment, not
/// a sentence that shows the word doing anything.
const MIN_BYTES: usize = 12;
/// Longest sentence worth storing. The scorer's length window already tails off
/// well before here; this only keeps a runaway paragraph out of the library.
const MAX_BYTES: usize = 320;

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

/// Does `text` contain `lemma` or a simple inflection of it?
pub fn mentions(text: &str, lemma: &str) -> bool {
    locate(&canonicalize(text), &fold_lemma(lemma)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(example: &FetchedExample) -> &str {
        &example.text[example.hl_start as usize..example.hl_end as usize]
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
}
