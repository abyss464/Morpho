//! Example fetching from the exam corpus.
//!
//! There is exactly one example source, and it is a local file. Without
//! `corpus_path` the rule derives nothing at all: no job, no dead letter, no
//! placeholder sentence. Every word then reports `missing_example`, which is
//! the honest description of a corpus that has not been supplied.

use std::sync::Arc;

use morpho_domain::job::{JobKey, JobKind, Priority, RateKey, SubjectRef};
use morpho_store::error::Result;

use crate::engine::EngineContext;
use crate::facts::{FetchKind, SOURCE_EXAM_CORPUS};
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
        if !self.context.sources.has_corpus() {
            return Ok(Vec::new());
        }
        let facts = snapshot.facts;
        let mut jobs = Vec::new();
        for word in &facts.active {
            if facts.fetched(&FetchKind::Examples, word.word_id, SOURCE_EXAM_CORPUS) {
                continue;
            }
            jobs.push(
                JobSpec::new(
                    JobKey::new(
                        JobKind::FetchExamples,
                        SubjectRef::word_source(word.word_id, SOURCE_EXAM_CORPUS),
                    ),
                    // A local file read; no external lane to protect.
                    RateKey::Cpu,
                    Priority::P2,
                )
                .with_tiebreak(word.frequency_rank, word.word_id)
                .with_payload(JobPayload::FetchExamples {
                    word_id: word.word_id,
                    lemma: word.lemma.clone(),
                }),
            );
        }
        Ok(jobs)
    }
}
