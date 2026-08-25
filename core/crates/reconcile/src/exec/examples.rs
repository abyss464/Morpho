//! Example fetching from the in-memory exam corpus.
//!
//! The corpus is loaded once at startup, so this executor does no I/O: it looks
//! the lemma up and commits whatever it finds, including nothing. Highlight
//! offsets were computed against the canonicalized sentence when the corpus was
//! indexed, which is the same text the store will hold.

use std::sync::Arc;

use async_trait::async_trait;

use morpho_domain::error::TaskError;
use morpho_domain::event::Actor;
use morpho_domain::job::JobKind;
use morpho_domain::types::ExampleSource;
use morpho_store::ops::IngestExamples;
use morpho_store::{Store, WriteOp};

use crate::engine::EngineContext;
use crate::exec::{store_error, wrong_payload, Executor};
use crate::rule::{JobPayload, JobSpec};

pub struct FetchExamplesExecutor {
    context: Arc<EngineContext>,
}

impl FetchExamplesExecutor {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Executor for FetchExamplesExecutor {
    fn kind(&self) -> JobKind {
        JobKind::FetchExamples
    }

    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError> {
        let JobPayload::FetchExamples { word_id, lemma } = &job.payload else {
            return Err(wrong_payload(JobKind::FetchExamples));
        };
        let Some(corpus) = self.context.sources.corpus.as_ref() else {
            return Err(TaskError::permanent("the exam corpus is not configured"));
        };

        let examples = corpus.examples(lemma).to_vec();
        store
            .write(
                Actor::Worker(JobKind::FetchExamples),
                WriteOp::IngestExamples(IngestExamples {
                    word_id: *word_id,
                    source: ExampleSource::ExamCorpus,
                    examples,
                }),
            )
            .await
            .map_err(store_error)?;
        Ok(())
    }
}
