//! Job executors.
//!
//! An executor performs one job and commits **one atomic** `WriteOp`: the
//! result rows and the input hash they were computed from land in the same
//! transaction, so there is never a window where a product exists without its
//! provenance (README Part 4 §"组件"). Executors hold no database connection
//! of their own — they go through the store handle like everyone else.

use async_trait::async_trait;

use morpho_domain::error::TaskError;
use morpho_domain::event::Actor;
use morpho_domain::job::JobKind;
use morpho_store::{Store, WriteOp, WriteResult};

use crate::rule::{JobPayload, JobSpec};
use crate::text::TextPipeline;

#[async_trait]
pub trait Executor: Send + Sync {
    fn kind(&self) -> JobKind;
    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError>;
}

/// Tokenizes one definition candidate and stores the result with its input hash.
pub struct ExtractTokensExecutor {
    pipeline: TextPipeline,
}

impl ExtractTokensExecutor {
    pub fn new(pipeline: TextPipeline) -> Self {
        Self { pipeline }
    }
}

#[async_trait]
impl Executor for ExtractTokensExecutor {
    fn kind(&self) -> JobKind {
        JobKind::ExtractTokens
    }

    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError> {
        let JobPayload::ExtractTokens {
            def_cand_id,
            text,
            text_hash,
        } = &job.payload
        else {
            return Err(TaskError::permanent(
                "extract_tokens job carried the wrong payload",
            ));
        };

        let tokens = self.pipeline.extract(text);
        let op = WriteOp::record_extraction(
            *def_cand_id,
            text_hash.clone(),
            self.pipeline.input_hash(text_hash),
            self.pipeline.tokenizer_ver().to_string(),
            self.pipeline.lemmatizer_ver().to_string(),
            tokens,
        );

        let outcome = store
            .write(Actor::Worker(JobKind::ExtractTokens), op)
            .await
            .map_err(|err| TaskError::transient(err.to_string()))?;

        if let WriteResult::Extraction { applied: false } = outcome.result {
            // The candidate changed underneath us; the next pass re-derives.
            tracing::debug!(def_cand_id, "extraction discarded: input drifted");
        }
        Ok(())
    }
}

/// The wave-1 executor set.
pub fn default_executors(pipeline: TextPipeline) -> Vec<std::sync::Arc<dyn Executor>> {
    vec![std::sync::Arc::new(ExtractTokensExecutor::new(pipeline))]
}
