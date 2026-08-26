//! Example fetching: the exam corpus, the Free Dictionary payload, Tatoeba.
//!
//! Ruling #18 gives examples three sources, and they reach the database by two
//! different routes.
//!
//! **Free Dictionary** is the odd one out, and deliberately so. Its per-sense
//! usage sentences arrive inside the *definition* payload, so
//! `FetchDefinitionsExecutor` commits both products in one transaction and both
//! completion markers with them — a word that has been asked for definitions
//! has already been asked for sentences, and asking again would double this
//! lane's traffic across six thousand words to learn nothing new. The job this
//! rule derives is therefore a *backfill*: it fires only for a word whose
//! definitions were fetched before the payload was being mined, which is
//! exactly the wave-2 and wave-3 rows already in the database. Fresh words
//! never reach it.
//!
//! **The exam corpus** is a local file: absent without `corpus_path`, and no
//! job is derived at all rather than a dead letter per word.
//!
//! **Tatoeba** needs no credentials, so it is always in the desired state until
//! it has answered once.

use std::sync::Arc;

use morpho_domain::job::{JobKey, JobKind, Priority, RateKey, SubjectRef};
use morpho_domain::types::ExampleSource;
use morpho_store::error::Result;
use morpho_store::queries::WordRow;

use crate::engine::EngineContext;
use crate::facts::{
    example_source_name, FetchKind, SOURCE_EXAM_CORPUS, SOURCE_FREEDICT, SOURCE_TATOEBA,
};
use crate::rule::{JobPayload, JobSpec, Rule, Snapshot};

pub struct FetchExamplesRule {
    context: Arc<EngineContext>,
}

impl FetchExamplesRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for FetchExamplesRule {
    fn name(&self) -> &'static str {
        "fetch_examples"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        let facts = snapshot.facts;
        let has_corpus = self.context.sources.has_corpus();
        let mut jobs = Vec::new();

        for word in &facts.active {
            if has_corpus && !facts.fetched(&FetchKind::Examples, word.word_id, SOURCE_EXAM_CORPUS)
            {
                // A local file read; no external lane to protect.
                jobs.push(job(word, ExampleSource::ExamCorpus, RateKey::Cpu));
            }

            // The backfill path described above: definitions already answered,
            // but no sentences came with them.
            if facts.fetched(&FetchKind::Definitions, word.word_id, SOURCE_FREEDICT)
                && !facts.fetched(&FetchKind::Examples, word.word_id, SOURCE_FREEDICT)
            {
                jobs.push(job(word, ExampleSource::Freedict, RateKey::Freedict));
            }

            if !facts.fetched(&FetchKind::Examples, word.word_id, SOURCE_TATOEBA) {
                jobs.push(job(word, ExampleSource::Tatoeba, RateKey::Tatoeba));
            }
        }
        Ok(jobs)
    }
}

fn job(word: &WordRow, source: ExampleSource, rate_key: RateKey) -> JobSpec {
    JobSpec::new(
        JobKey::new(
            JobKind::FetchExamples,
            SubjectRef::word_source(word.word_id, example_source_name(source)),
        ),
        rate_key,
        // P2: backlog backfill, ordered by frequency so the words a learner
        // meets first become shippable first.
        Priority::P2,
    )
    .with_tiebreak(word.frequency_rank, word.word_id)
    .with_payload(JobPayload::FetchExamples {
        word_id: word.word_id,
        lemma: word.lemma.clone(),
        source,
    })
}
