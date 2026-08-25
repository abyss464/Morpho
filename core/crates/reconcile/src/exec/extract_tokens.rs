//! Tokenizes one definition candidate and stores the result with its input
//! hash, in a single transaction.

use async_trait::async_trait;

use morpho_domain::error::TaskError;
use morpho_domain::event::Actor;
use morpho_domain::job::JobKind;
use morpho_store::{Store, WriteOp, WriteResult};

use crate::exec::{store_error, wrong_payload, Executor};
use crate::rule::{JobPayload, JobSpec};
use crate::text::TextPipeline;

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
            return Err(wrong_payload(JobKind::ExtractTokens));
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
            .map_err(store_error)?;

        if let WriteResult::Extraction { applied: false } = outcome.result {
            // The candidate changed underneath us; the next pass re-derives.
            tracing::debug!(def_cand_id, "extraction discarded: input drifted");
        }
        Ok(())
    }
}
