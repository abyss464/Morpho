//! Definition fetching: Free Dictionary first, WordNet as the fallback.
//!
//! Desired state per active word: at least one source has been asked. The Free
//! Dictionary is always asked, because it needs no credentials. WordNet is only
//! asked once the Free Dictionary is exhausted *and* the word still has no
//! candidate — a gloss is a weaker definition than a real dictionary entry, so
//! it is a fallback, not a second opinion.
//!
//! With no `wordnet_dir` configured the fallback is absent, and a word the Free
//! Dictionary does not know honestly reports `missing_definition`.

use std::sync::Arc;

use morpho_domain::job::{JobKey, JobKind, Priority, RateKey, SubjectRef};
use morpho_domain::types::DefinitionSource;
use morpho_store::error::Result;

use crate::engine::EngineContext;
use crate::facts::{definition_source_name, FetchKind, SOURCE_FREEDICT, SOURCE_WORDNET};
use crate::rule::{JobPayload, JobSpec, Rule, Snapshot};

pub struct FetchDefinitionsRule {
    context: Arc<EngineContext>,
}

impl FetchDefinitionsRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for FetchDefinitionsRule {
    fn name(&self) -> &'static str {
        "fetch_definitions"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        let facts = snapshot.facts;
        let wordnet_available = self.context.sources.has_wordnet();
        let mut jobs = Vec::new();

        for word in &facts.active {
            // Primary source. It has no key and no config, so it is always in
            // the desired state until it has answered once.
            if !facts.fetched(&FetchKind::Definitions, word.word_id, SOURCE_FREEDICT) {
                jobs.push(job(word, DefinitionSource::Freedict, RateKey::Freedict));
            }

            if !wordnet_available {
                continue;
            }
            if facts.words_with_definitions.contains(&word.word_id) {
                // Something already answered; a gloss would only add noise.
                continue;
            }
            if !facts.source_exhausted(
                &FetchKind::Definitions,
                JobKind::FetchDefinitions,
                word.word_id,
                SOURCE_FREEDICT,
            ) {
                continue;
            }
            if facts.fetched(&FetchKind::Definitions, word.word_id, SOURCE_WORDNET) {
                continue;
            }
            jobs.push(job(word, DefinitionSource::Wordnet, RateKey::Cpu));
        }
        Ok(jobs)
    }
}

fn job(
    word: &morpho_store::queries::WordRow,
    source: DefinitionSource,
    rate_key: RateKey,
) -> JobSpec {
    JobSpec::new(
        JobKey::new(
            JobKind::FetchDefinitions,
            SubjectRef::word_source(word.word_id, definition_source_name(source)),
        ),
        rate_key,
        // P2: backlog backfill, ordered by frequency so the words a learner
        // meets first become shippable first.
        Priority::P2,
    )
    .with_tiebreak(word.frequency_rank, word.word_id)
    .with_payload(JobPayload::FetchDefinitions {
        word_id: word.word_id,
        lemma: word.lemma.clone(),
        source,
    })
}
