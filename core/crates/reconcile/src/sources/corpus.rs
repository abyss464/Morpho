//! The exam-corpus example source.
//!
//! ## File format
//!
//! `corpus_path` points at a JSONL file — one JSON object per line:
//!
//! ```json
//! {"word": "benevolent", "sentence": "The benevolent donor funded the library."}
//! {"word": "adapt", "sentence": "Species adapt to a changing climate.", "source": "2019-T1"}
//! ```
//!
//! * `word` — the lemma the sentence illustrates, matched case-insensitively;
//! * `sentence` — the sentence itself, stored canonicalized;
//! * `source` — optional provenance (paper, year, question number), kept in
//!   `example_candidates.source_ref`.
//!
//! Blank lines and `#` comments are skipped. Unknown keys are ignored so a
//! richer corpus file still loads.
//!
//! **Highlight offsets are computed after canonicalization**, because that is
//! what gets stored and what the app renders (`working-db.sql`
//! `example_candidates.hl_start`). A sentence in which the target word cannot be
//! located is dropped rather than stored with a guessed range — a wrong
//! highlight is worse than a missing example.
//!
//! Without `corpus_path` this source is absent, no example jobs are derived,
//! and every word honestly reports `missing_example`. Nothing is fabricated.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use morpho_domain::canon::{canonicalize, fold_lemma};
use morpho_domain::types::FetchedExample;

/// Sentences kept per word. The selection layer only ever fills three slots.
pub const MAX_PER_WORD: usize = 8;

#[derive(Debug, Deserialize)]
struct CorpusRow {
    #[serde(alias = "lemma")]
    word: String,
    #[serde(alias = "text", alias = "example")]
    sentence: String,
    #[serde(default)]
    source: Option<String>,
}

/// The corpus, indexed by folded lemma.
#[derive(Debug, Default)]
pub struct ExamCorpus {
    by_lemma: HashMap<String, Vec<FetchedExample>>,
    sentence_count: usize,
    skipped: usize,
}

impl ExamCorpus {
    /// Read and index a corpus file.
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let corpus = Self::parse(&raw);
        tracing::info!(
            path = %path.display(),
            lemmas = corpus.by_lemma.len(),
            sentences = corpus.sentence_count,
            skipped = corpus.skipped,
            "loaded exam corpus"
        );
        Ok(corpus)
    }

    /// Parse an in-memory corpus.
    pub fn parse(raw: &str) -> Self {
        let mut corpus = Self::default();
        for (number, line) in raw.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let row: CorpusRow = match serde_json::from_str(line) {
                Ok(row) => row,
                Err(err) => {
                    tracing::warn!(line = number + 1, error = %err, "skipping corpus row");
                    corpus.skipped += 1;
                    continue;
                }
            };
            let lemma = fold_lemma(&row.word);
            let text = canonicalize(&row.sentence);
            if lemma.is_empty() || text.is_empty() {
                corpus.skipped += 1;
                continue;
            }
            let Some((hl_start, hl_end)) = locate(&text, &lemma) else {
                // The sentence does not contain the word it claims to
                // illustrate: unusable for mode 1, so it is not stored.
                tracing::debug!(
                    line = number + 1,
                    lemma,
                    "corpus row has no locatable target"
                );
                corpus.skipped += 1;
                continue;
            };
            let bucket = corpus.by_lemma.entry(lemma).or_default();
            if bucket.iter().any(|existing| existing.text == text) {
                continue;
            }
            if bucket.len() >= MAX_PER_WORD {
                continue;
            }
            bucket.push(FetchedExample {
                text,
                hl_start: hl_start as i64,
                hl_end: hl_end as i64,
                source_ref: row
                    .source
                    .map(|s| format!("exam_corpus:{s}"))
                    .or_else(|| Some("exam_corpus".to_string())),
            });
            corpus.sentence_count += 1;
        }
        corpus
    }

    pub fn is_empty(&self) -> bool {
        self.sentence_count == 0
    }

    pub fn lemma_count(&self) -> usize {
        self.by_lemma.len()
    }

    pub fn sentence_count(&self) -> usize {
        self.sentence_count
    }

    /// Sentences for one lemma. An unknown lemma yields an empty slice, which
    /// is a legitimate zero-result fetch, not a failure.
    pub fn examples(&self, lemma: &str) -> &[FetchedExample] {
        self.by_lemma
            .get(&fold_lemma(lemma))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

/// Byte range of `lemma` inside the already-canonicalized `text`.
///
/// Matching is case-insensitive and whole-word, and prefers the exact form
/// before falling back to a common inflection, so "adapt" highlights `adapt`
/// in "Species adapt…" and `adapted` in "Species adapted…".
pub fn locate(text: &str, lemma: &str) -> Option<(usize, usize)> {
    let lower = text.to_lowercase();
    let needle = lemma.to_lowercase();
    if needle.is_empty() {
        return None;
    }

    // Exact word first.
    if let Some(range) = find_word(&lower, &needle) {
        return Some(range);
    }
    // Then the regular English inflections, longest first so "-ies" wins over
    // "-s". These are surface forms of the same lemma, not different words.
    for suffix in ["ies", "ing", "ied", "ees", "es", "ed", "en", "er", "s", "d"] {
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

    const SAMPLE: &str = r#"
# Morpho exam corpus fixture
{"word": "benevolent", "sentence": "The  benevolent   donor funded the library.", "source": "2019-T1"}
{"word": "adapt", "sentence": "Species adapted to a changing climate."}
{"word": "serene", "sentence": "A serene lake lay below."}
{"word": "serene", "sentence": "A serene lake lay below."}
{"word": "absent", "sentence": "This sentence never mentions it."}
not json at all
"#;

    #[test]
    fn parses_a_corpus_and_indexes_by_lemma() {
        let corpus = ExamCorpus::parse(SAMPLE);
        assert_eq!(corpus.lemma_count(), 3);
        assert_eq!(corpus.sentence_count(), 3);
        assert!(!corpus.is_empty());
    }

    #[test]
    fn text_is_canonicalized_before_offsets_are_taken() {
        let corpus = ExamCorpus::parse(SAMPLE);
        let example = &corpus.examples("benevolent")[0];
        assert_eq!(example.text, "The benevolent donor funded the library.");
        let slice = &example.text[example.hl_start as usize..example.hl_end as usize];
        assert_eq!(slice, "benevolent");
    }

    #[test]
    fn offsets_index_the_stored_text_exactly() {
        let corpus = ExamCorpus::parse(SAMPLE);
        for lemma in ["benevolent", "adapt", "serene"] {
            for example in corpus.examples(lemma) {
                let slice = &example.text[example.hl_start as usize..example.hl_end as usize];
                assert!(
                    slice.to_lowercase().starts_with(lemma),
                    "{lemma}: highlighted {slice:?}"
                );
            }
        }
    }

    #[test]
    fn inflected_forms_are_highlighted() {
        let corpus = ExamCorpus::parse(SAMPLE);
        let example = &corpus.examples("adapt")[0];
        let slice = &example.text[example.hl_start as usize..example.hl_end as usize];
        assert_eq!(slice, "adapted");
    }

    #[test]
    fn a_sentence_without_the_word_is_dropped() {
        let corpus = ExamCorpus::parse(SAMPLE);
        assert!(corpus.examples("absent").is_empty());
    }

    #[test]
    fn malformed_rows_are_counted_and_skipped() {
        let corpus = ExamCorpus::parse(SAMPLE);
        // one unparsable line + one unlocatable sentence
        assert_eq!(corpus.skipped, 2);
    }

    #[test]
    fn duplicate_sentences_collapse() {
        let corpus = ExamCorpus::parse(SAMPLE);
        assert_eq!(corpus.examples("serene").len(), 1);
    }

    #[test]
    fn provenance_is_preserved() {
        let corpus = ExamCorpus::parse(SAMPLE);
        assert_eq!(
            corpus.examples("benevolent")[0].source_ref.as_deref(),
            Some("exam_corpus:2019-T1")
        );
        assert_eq!(
            corpus.examples("adapt")[0].source_ref.as_deref(),
            Some("exam_corpus")
        );
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let corpus = ExamCorpus::parse(SAMPLE);
        assert_eq!(corpus.examples("SERENE").len(), 1);
        assert_eq!(corpus.examples("Serene").len(), 1);
    }

    #[test]
    fn an_unknown_lemma_yields_an_empty_slice() {
        let corpus = ExamCorpus::parse(SAMPLE);
        assert!(corpus.examples("copious").is_empty());
    }

    #[test]
    fn an_empty_corpus_is_empty_not_an_error() {
        let corpus = ExamCorpus::parse("");
        assert!(corpus.is_empty());
        assert_eq!(corpus.lemma_count(), 0);
    }

    #[test]
    fn caps_sentences_per_word() {
        let rows: Vec<String> = (0..20)
            .map(|i| format!(r#"{{"word":"serene","sentence":"A serene number {i} lake."}}"#))
            .collect();
        let corpus = ExamCorpus::parse(&rows.join("\n"));
        assert_eq!(corpus.examples("serene").len(), MAX_PER_WORD);
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
    fn handles_stem_changing_inflections() {
        assert!(locate("The lake is serener today.", "serene").is_some());
        assert!(locate("Species are adapting fast.", "adapt").is_some());
    }

    #[test]
    fn offsets_land_on_character_boundaries_with_accents() {
        let text = canonicalize("A café and a serene view.");
        let (start, end) = locate(&text, "serene").unwrap();
        assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        assert_eq!(&text[start..end], "serene");
    }

    #[test]
    fn an_empty_lemma_never_matches() {
        assert_eq!(locate("anything", ""), None);
    }

    #[test]
    fn alternative_field_names_are_accepted() {
        let corpus =
            ExamCorpus::parse(r#"{"lemma":"serene","text":"A serene lake.","source":"x"}"#);
        assert_eq!(corpus.examples("serene").len(), 1);
    }
}
