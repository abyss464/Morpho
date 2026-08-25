//! Tokenization and lemmatization, behind traits.
//!
//! Wave 1 ships deliberately naive implementations: a whitespace/punctuation
//! tokenizer and a lowercase "lemmatizer". Both are versioned, and the version
//! feeds `def_extractions.input_hash` — swapping in real morphology later
//! invalidates exactly the extractions it should, with no migration
//! (README Part 3 §"哈希覆盖一览").

use std::sync::Arc;

use morpho_domain::canon::fold_lemma;
use morpho_domain::hash::def_extraction_input_hash;
use morpho_domain::types::ExtractedToken;

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

/// Word-boundary tokenizer.
///
/// A token is a run of alphabetic characters, allowing an interior apostrophe
/// (`don't`). Everything else — digits, hyphens, punctuation — is a boundary,
/// so `well-meaning` yields two tokens, which is what the dependency graph
/// wants: the reader must know both halves.
#[derive(Debug, Default, Clone, Copy)]
pub struct SimpleTokenizer;

const SIMPLE_TOKENIZER_VER: &str = "simple-tokenizer/1";

impl Tokenizer for SimpleTokenizer {
    fn version(&self) -> &str {
        SIMPLE_TOKENIZER_VER
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut tokens = Vec::new();
        let mut current = String::new();
        for (i, ch) in chars.iter().enumerate() {
            if ch.is_alphabetic() {
                current.push(*ch);
                continue;
            }
            let interior_apostrophe = matches!(ch, '\'' | '\u{2019}')
                && !current.is_empty()
                && chars.get(i + 1).is_some_and(|next| next.is_alphabetic());
            if interior_apostrophe {
                current.push('\'');
                continue;
            }
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            tokens.push(current);
        }
        tokens
    }
}

/// Placeholder lemmatizer: case folding only.
///
/// Real inflectional analysis (WordNet morphy / Morfessor) arrives later; the
/// version bump is what makes every extraction recompute.
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
#[derive(Clone)]
pub struct TextPipeline {
    tokenizer: Arc<dyn Tokenizer>,
    lemmatizer: Arc<dyn Lemmatizer>,
}

impl Default for TextPipeline {
    fn default() -> Self {
        Self {
            tokenizer: Arc::new(SimpleTokenizer),
            lemmatizer: Arc::new(LowercaseLemmatizer),
        }
    }
}

impl TextPipeline {
    pub fn new(tokenizer: Arc<dyn Tokenizer>, lemmatizer: Arc<dyn Lemmatizer>) -> Self {
        Self {
            tokenizer,
            lemmatizer,
        }
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
        assert_eq!(surfaces("the dogs' bowls"), ["the", "dogs", "bowls"]);
        assert_eq!(surfaces("'quoted'"), ["quoted"]);
        assert_eq!(surfaces("it\u{2019}s"), ["it's"]);
    }

    #[test]
    fn drops_digits_and_symbols() {
        assert_eq!(surfaces("top 10 (ten) items"), ["top", "ten", "items"]);
        assert_eq!(surfaces("---"), Vec::<String>::new());
        assert_eq!(surfaces(""), Vec::<String>::new());
    }

    #[test]
    fn lemmatizer_folds_case_only() {
        assert_eq!(LowercaseLemmatizer.lemmatize("Kindly"), "kindly");
        assert_eq!(LowercaseLemmatizer.lemmatize("MEANING"), "meaning");
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
    fn input_hash_tracks_tool_versions() {
        struct OtherTokenizer;
        impl Tokenizer for OtherTokenizer {
            fn version(&self) -> &str {
                "simple-tokenizer/2"
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
}
