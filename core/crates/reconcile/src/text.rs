//! Tokenization and lemmatization, behind traits.
//!
//! Both halves are versioned, and the version feeds
//! `def_extractions.input_hash` — bumping one invalidates exactly the
//! extractions it should, with no migration (README Part 3 §"哈希覆盖一览").
//!
//! Wave 5 replaced the placeholder lowercase "lemmatizer" with [`crate::morphy`],
//! which is why both versions moved: the tokenizer now strips possessives and
//! drops a small allowlist of abbreviations, and the lemmatizer does real
//! inflectional analysis against the lexicon.

use std::sync::Arc;

use morpho_domain::canon::fold_lemma;
use morpho_domain::hash::def_extraction_input_hash;
use morpho_domain::types::ExtractedToken;

use crate::lexicon::LexiconCache;
use crate::morphy::{ExceptionTable, MorphyLemmatizer};

/// Splits definition text into ordered surface forms.
pub trait Tokenizer: Send + Sync {
    /// Version string stored in `def_extractions.tokenizer_ver`.
    fn version(&self) -> &str;
    fn tokenize(&self, text: &str) -> Vec<String>;
}

/// Reduces a surface form to the lexicon key used for word lookups.
pub trait Lemmatizer: Send + Sync {
    /// Version string stored in `def_extractions.lemmatizer_ver`.
    fn version(&self) -> &str;
    fn lemmatize(&self, surface: &str) -> String;
}

/// Abbreviations a Chinese junior-high graduate reads without help.
///
/// Dictionary-definition furniture: they carry no meaning the reader has to be
/// taught, and they are not English words, so they are dropped from the token
/// stream rather than emitted. That makes them behave like `base` vocabulary —
/// no dependency edge, no OOV entry — which is the only sense in which the
/// current schema can call a token "already known" without inventing a `words`
/// row for it. Emitting them instead would be worse than useless: `e.g.` splits
/// into the tokens `e` and `g`.
///
/// Ordered longest first; matching is case-insensitive and requires a
/// non-alphabetic character after the match, so `sth` never eats `sthenic`.
/// The list is part of the tokenizer version: adding to it re-extracts.
pub const ABBREVIATIONS: &[&str] = &[
    "e.g.", "i.e.", "etc.", "vs.", "e.g", "i.e", "etc", "sth", "vs", "sb",
];

/// Word-boundary tokenizer.
///
/// A token is a run of alphabetic characters, allowing an interior apostrophe
/// (`don't`). Everything else — digits, hyphens, punctuation — is a boundary,
/// so `well-meaning` yields two tokens, which is what the dependency graph
/// wants: the reader must know both halves.
///
/// Two things are normalized away rather than passed to the lemmatizer, because
/// they are orthography rather than morphology:
///
/// * the possessive clitic — `one's` is the token `one`, and `the dogs' bowls`
///   is `dogs` (the trailing apostrophe is already a boundary);
/// * the entries in [`ABBREVIATIONS`], which are skipped entirely.
#[derive(Debug, Default, Clone, Copy)]
pub struct SimpleTokenizer;

const SIMPLE_TOKENIZER_VER: &str = "simple-tokenizer/2";

impl Tokenizer for SimpleTokenizer {
    fn version(&self) -> &str {
        SIMPLE_TOKENIZER_VER
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if !chars[i].is_alphabetic() {
                i += 1;
                continue;
            }
            if let Some(len) = abbreviation_at(&chars, i) {
                i += len;
                continue;
            }
            let start = i;
            while i < chars.len() {
                if chars[i].is_alphabetic() {
                    i += 1;
                    continue;
                }
                let interior_apostrophe = is_apostrophe(chars[i])
                    && i > start
                    && chars.get(i + 1).is_some_and(|next| next.is_alphabetic());
                if !interior_apostrophe {
                    break;
                }
                i += 1;
            }
            let raw: String = chars[start..i]
                .iter()
                .map(|ch| if is_apostrophe(*ch) { '\'' } else { *ch })
                .collect();
            let token = strip_possessive(raw);
            if !token.is_empty() {
                tokens.push(token);
            }
        }
        tokens
    }
}

fn is_apostrophe(ch: char) -> bool {
    matches!(ch, '\'' | '\u{2019}')
}

/// `one's` → `one`. `don't` is left alone: the clitic is `'s`, not any `'`.
fn strip_possessive(token: String) -> String {
    match token.strip_suffix("'s") {
        Some(stem) if !stem.is_empty() => stem.to_string(),
        _ => token,
    }
}

/// Length in `char`s of the abbreviation starting at `start`, if any.
fn abbreviation_at(chars: &[char], start: usize) -> Option<usize> {
    'next: for abbreviation in ABBREVIATIONS {
        let pattern: Vec<char> = abbreviation.chars().collect();
        let end = start + pattern.len();
        if end > chars.len() {
            continue;
        }
        for (expected, actual) in pattern.iter().zip(&chars[start..end]) {
            if !actual.eq_ignore_ascii_case(expected) {
                continue 'next;
            }
        }
        // A trailing letter or clitic means this was the head of a real word.
        match chars.get(end) {
            Some(ch) if ch.is_alphabetic() || is_apostrophe(*ch) => continue,
            _ => return Some(pattern.len()),
        }
    }
    None
}

/// Placeholder lemmatizer: case folding only.
///
/// Superseded by [`MorphyLemmatizer`] in wave 5 and kept because it is the
/// clearest way to state "this pipeline does no morphology" — the exporter
/// tests and the version-bump tests both want that.
#[derive(Debug, Default, Clone, Copy)]
pub struct LowercaseLemmatizer;

const LOWERCASE_LEMMATIZER_VER: &str = "lowercase-lemmatizer/1";

impl Lemmatizer for LowercaseLemmatizer {
    fn version(&self) -> &str {
        LOWERCASE_LEMMATIZER_VER
    }

    fn lemmatize(&self, surface: &str) -> String {
        fold_lemma(surface)
    }
}

/// The tokenizer/lemmatizer pair shared by the `ExtractTokens` rule and its
/// executor. They must agree, or the rule would re-derive the job that the
/// executor just satisfied.
///
/// Cloning shares the lexicon cache, so "agree" also covers the lemma set.
#[derive(Clone)]
pub struct TextPipeline {
    tokenizer: Arc<dyn Tokenizer>,
    lemmatizer: Arc<dyn Lemmatizer>,
    lexicon: Arc<LexiconCache>,
}

impl Default for TextPipeline {
    /// Morphy over the compiled-in irregular table, with an empty lexicon until
    /// the first sweep refreshes it.
    fn default() -> Self {
        Self::with_exceptions(Arc::new(ExceptionTable::builtin()))
    }
}

impl TextPipeline {
    /// Explicit parts, over a fresh cache.
    ///
    /// The lemmatizer keeps whatever lexicon it was built with, so a caller
    /// pairing this with a [`MorphyLemmatizer`] must hand the same cache back
    /// through [`TextPipeline::with_lexicon`] — otherwise the sweep refreshes a
    /// cache nobody reads.
    pub fn new(tokenizer: Arc<dyn Tokenizer>, lemmatizer: Arc<dyn Lemmatizer>) -> Self {
        Self {
            tokenizer,
            lemmatizer,
            lexicon: Arc::new(LexiconCache::new()),
        }
    }

    /// Point the pipeline at the cache its lemmatizer actually reads.
    #[must_use]
    pub fn with_lexicon(mut self, lexicon: Arc<LexiconCache>) -> Self {
        self.lexicon = lexicon;
        self
    }

    /// Morphy with WNdb's exception files when `wordnet_dir` points at a real
    /// dictionary, and the compiled-in table otherwise.
    ///
    /// The two report different `lemmatizer_ver` strings, so turning WordNet on
    /// or off re-extracts rather than silently mixing two analyses.
    pub fn from_wordnet_dir(dir: Option<&std::path::Path>) -> Self {
        let exceptions = match dir {
            Some(dir) => ExceptionTable::layered(dir),
            None => ExceptionTable::builtin(),
        };
        Self::with_exceptions(Arc::new(exceptions))
    }

    fn with_exceptions(exceptions: Arc<ExceptionTable>) -> Self {
        let lexicon = Arc::new(LexiconCache::new());
        Self {
            tokenizer: Arc::new(SimpleTokenizer),
            lemmatizer: Arc::new(MorphyLemmatizer::new(exceptions, lexicon.clone())),
            lexicon,
        }
    }

    /// The lemma set the lemmatizer validates against. Refreshed once per pass
    /// by the maintenance sweep.
    pub fn lexicon(&self) -> Arc<LexiconCache> {
        self.lexicon.clone()
    }

    pub fn tokenizer_ver(&self) -> &str {
        self.tokenizer.version()
    }

    pub fn lemmatizer_ver(&self) -> &str {
        self.lemmatizer.version()
    }

    /// Hash recorded in `def_extractions.input_hash` for a candidate.
    pub fn input_hash(&self, text_hash: &str) -> String {
        def_extraction_input_hash(text_hash, self.tokenizer_ver(), self.lemmatizer_ver())
    }

    /// Tokenize and lemmatize one definition text.
    pub fn extract(&self, text: &str) -> Vec<ExtractedToken> {
        self.tokenizer
            .tokenize(text)
            .into_iter()
            .enumerate()
            .map(|(position, surface)| ExtractedToken {
                lemma: self.lemmatizer.lemmatize(&surface),
                position: position as i64,
                surface,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surfaces(text: &str) -> Vec<String> {
        SimpleTokenizer.tokenize(text)
    }

    #[test]
    fn splits_on_whitespace_and_punctuation() {
        assert_eq!(
            surfaces("well meaning and kindly"),
            ["well", "meaning", "and", "kindly"]
        );
        assert_eq!(
            surfaces("To give up, completely; forever."),
            ["To", "give", "up", "completely", "forever"]
        );
    }

    #[test]
    fn splits_hyphenated_compounds() {
        assert_eq!(
            surfaces("a well-meaning act"),
            ["a", "well", "meaning", "act"]
        );
    }

    #[test]
    fn keeps_interior_apostrophes() {
        assert_eq!(surfaces("don't"), ["don't"]);
        assert_eq!(surfaces("'quoted'"), ["quoted"]);
        assert_eq!(surfaces("o'clock"), ["o'clock"]);
    }

    #[test]
    fn strips_the_possessive_clitic() {
        assert_eq!(surfaces("one's own"), ["one", "own"]);
        assert_eq!(surfaces("the dogs' bowls"), ["the", "dogs", "bowls"]);
        assert_eq!(surfaces("it\u{2019}s"), ["it"]);
        assert_eq!(surfaces("a person\u{2019}s name"), ["a", "person", "name"]);
        // Not every clitic is possessive, and none of the others end in `'s`.
        assert_eq!(
            surfaces("they don't; we won't"),
            ["they", "don't", "we", "won't"]
        );
    }

    #[test]
    fn drops_the_abbreviation_allowlist() {
        assert_eq!(
            surfaces("a fruit, e.g. an apple"),
            ["a", "fruit", "an", "apple"]
        );
        assert_eq!(
            surfaces("that is, i.e., the same"),
            ["that", "is", "the", "same"]
        );
        assert_eq!(surfaces("apples, pears, etc."), ["apples", "pears"]);
        assert_eq!(surfaces("to tell sb sth"), ["to", "tell"]);
        assert_eq!(surfaces("one vs. the other"), ["one", "the", "other"]);
    }

    #[test]
    fn the_allowlist_needs_a_word_boundary() {
        assert_eq!(surfaces("sthenic"), ["sthenic"]);
        assert_eq!(surfaces("etching"), ["etching"]);
        assert_eq!(surfaces("sberbank"), ["sberbank"]);
        assert_eq!(surfaces("vsync"), ["vsync"]);
    }

    #[test]
    fn drops_digits_and_symbols() {
        assert_eq!(surfaces("top 10 (ten) items"), ["top", "ten", "items"]);
        assert_eq!(surfaces("---"), Vec::<String>::new());
        assert_eq!(surfaces(""), Vec::<String>::new());
    }

    #[test]
    fn lowercase_lemmatizer_folds_case_only() {
        assert_eq!(LowercaseLemmatizer.lemmatize("Kindly"), "kindly");
        assert_eq!(LowercaseLemmatizer.lemmatize("HAVING"), "having");
    }

    #[test]
    fn positions_are_dense_and_ordered() {
        let pipeline = TextPipeline::default();
        let tokens = pipeline.extract("Well meaning AND kindly");
        assert_eq!(tokens.len(), 4);
        for (i, token) in tokens.iter().enumerate() {
            assert_eq!(token.position, i as i64);
        }
        assert_eq!(tokens[0].surface, "Well");
        assert_eq!(tokens[0].lemma, "well");
        assert_eq!(tokens[2].lemma, "and");
    }

    #[test]
    fn the_default_pipeline_lemmatizes_against_its_lexicon() {
        let pipeline = TextPipeline::default();
        // Nothing is known yet, so every token stays out of scope.
        assert_eq!(
            pipeline
                .extract("having made")
                .iter()
                .map(|t| t.lemma.clone())
                .collect::<Vec<_>>(),
            ["having", "made"]
        );
        pipeline
            .lexicon()
            .replace(["have", "make"].into_iter().collect());
        assert_eq!(
            pipeline
                .extract("having made")
                .iter()
                .map(|t| t.lemma.clone())
                .collect::<Vec<_>>(),
            ["have", "make"]
        );
    }

    #[test]
    fn the_shipped_versions_are_the_wave_five_ones() {
        let pipeline = TextPipeline::default();
        assert_eq!(pipeline.tokenizer_ver(), "simple-tokenizer/2");
        assert_eq!(pipeline.lemmatizer_ver(), "morphy-lemmatizer/1");
    }

    #[test]
    fn input_hash_tracks_tool_versions() {
        struct OtherTokenizer;
        impl Tokenizer for OtherTokenizer {
            fn version(&self) -> &str {
                "simple-tokenizer/3"
            }
            fn tokenize(&self, text: &str) -> Vec<String> {
                SimpleTokenizer.tokenize(text)
            }
        }

        let base = TextPipeline::default();
        let bumped = TextPipeline::new(Arc::new(OtherTokenizer), Arc::new(LowercaseLemmatizer));
        let text_hash = morpho_domain::hash::text_hash("well meaning and kindly");
        assert_ne!(base.input_hash(&text_hash), bumped.input_hash(&text_hash));
        assert_eq!(base.input_hash(&text_hash), base.input_hash(&text_hash));
    }

    /// The whole wave-5 invalidation: same text, different lemmatizer version.
    #[test]
    fn the_lemmatizer_upgrade_changes_every_input_hash() {
        let old = TextPipeline::new(Arc::new(SimpleTokenizer), Arc::new(LowercaseLemmatizer));
        let new = TextPipeline::default();
        let text_hash = morpho_domain::hash::text_hash("having made a promise");
        assert_ne!(old.lemmatizer_ver(), new.lemmatizer_ver());
        assert_ne!(old.input_hash(&text_hash), new.input_hash(&text_hash));
    }
}
