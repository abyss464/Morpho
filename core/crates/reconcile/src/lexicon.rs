//! The lemma set the lemmatizer validates against.
//!
//! Morphy's detachment rules are generative: they will happily turn `number`
//! into `numb` or `havings` into `having`. WordNet keeps them honest by only
//! accepting a candidate that its own index knows. Morpho's index is the
//! `words` table, so that is what a candidate is checked against — the
//! tokenizer's job is matching definition tokens to words the reader will
//! actually meet, not general-purpose morphology.
//!
//! Membership must agree with the join in `def_dependencies` and
//! `oos_occurrences`, which compare `def_tokens.lemma` to `words.lemma` under
//! `COLLATE NOCASE`. [`morpho_domain::canon::fold_lemma`] is that comparison in
//! Rust, so every key stored here and every key looked up goes through it.
//!
//! Every role counts. A `base` word is not a dependency edge, but a token that
//! matches one is not out of scope either, and both views agree on that.
//!
//! ## Freshness
//!
//! [`LexiconCache`] is refreshed once per reconcile pass, before any stage or
//! rule reads it (`stages::run`). It is deliberately *not* mixed into
//! `def_extractions.input_hash`: the hash covers the candidate text and the
//! tool versions, and folding a whole-table fingerprint into it would re-run
//! every extraction on every single word import. The drift that leaves is
//! narrow — a token whose *detached* lemma is added to the lexicon later — and
//! it costs a stale cache entry, not a wrong one, because promoting the token's
//! own surface (what resolving an OOV entry actually does) is picked up by the
//! pure views with no extraction at all.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use morpho_domain::canon::fold_lemma;
use morpho_store::error::Result;

/// An immutable snapshot of every `words.lemma`, folded.
#[derive(Debug, Default)]
pub struct Lexicon {
    lemmas: HashSet<String>,
}

impl Lexicon {
    /// Read every lemma, whatever its role.
    pub fn load(conn: &rusqlite::Connection) -> Result<Self> {
        let mut stmt = conn.prepare_cached("SELECT lemma FROM words")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().collect())
    }

    /// Is this a lemma the reader can be sent to?
    pub fn contains(&self, folded: &str) -> bool {
        self.lemmas.contains(folded)
    }

    pub fn len(&self) -> usize {
        self.lemmas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lemmas.is_empty()
    }
}

impl<S: AsRef<str>> FromIterator<S> for Lexicon {
    fn from_iter<I: IntoIterator<Item = S>>(iter: I) -> Self {
        Self {
            lemmas: iter.into_iter().map(|s| fold_lemma(s.as_ref())).collect(),
        }
    }
}

/// The live handle the lemmatizer holds.
///
/// Cloning a [`crate::TextPipeline`] shares the cache, so the rule and the
/// executor that must agree on an extraction cannot disagree about the lexicon
/// either.
#[derive(Debug, Default)]
pub struct LexiconCache {
    current: RwLock<Arc<Lexicon>>,
}

impl LexiconCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-populated cache, for tests and for callers that already have the set.
    pub fn seeded<S: AsRef<str>>(lemmas: impl IntoIterator<Item = S>) -> Self {
        let cache = Self::new();
        cache.replace(lemmas.into_iter().collect());
        cache
    }

    pub fn snapshot(&self) -> Arc<Lexicon> {
        self.current.read().expect("lexicon cache poisoned").clone()
    }

    pub fn replace(&self, lexicon: Lexicon) {
        *self.current.write().expect("lexicon cache poisoned") = Arc::new(lexicon);
    }

    /// Re-read the whole table. Called once per pass; a full `SELECT lemma` over
    /// six thousand rows is cheaper than the round trip that would avoid it.
    pub fn refresh(&self, conn: &rusqlite::Connection) -> Result<usize> {
        let lexicon = Lexicon::load(conn)?;
        let len = lexicon.len();
        self.replace(lexicon);
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership_ignores_case_and_surrounding_space() {
        let lexicon: Lexicon = ["Have", "  make  "].into_iter().collect();
        assert!(lexicon.contains("have"));
        assert!(lexicon.contains("make"));
        assert!(!lexicon.contains("having"));
    }

    #[test]
    fn an_empty_cache_knows_nothing() {
        let cache = LexiconCache::new();
        assert!(cache.snapshot().is_empty());
        assert!(!cache.snapshot().contains("have"));
    }

    #[test]
    fn replacing_is_visible_to_snapshots_taken_afterwards() {
        let cache = LexiconCache::new();
        let before = cache.snapshot();
        cache.replace(["have"].into_iter().collect());
        assert!(!before.contains("have"), "old snapshots stay immutable");
        assert!(cache.snapshot().contains("have"));
    }
}
