//! Job executors.
//!
//! An executor performs one job and commits **one atomic** `WriteOp`: the
//! result rows and the input hash or completion marker they were computed from
//! land in the same transaction, so there is never a window where a product
//! exists without its provenance (README Part 4 §"组件"). Executors hold no
//! database connection of their own — they go through the store handle like
//! everyone else.

mod definitions;
mod etymology;
mod examples;
mod extract_tokens;
mod images;
mod tts;

use std::sync::Arc;

use async_trait::async_trait;

use morpho_domain::error::TaskError;
use morpho_domain::job::JobKind;
use morpho_store::Store;

use crate::engine::EngineContext;
use crate::rule::JobSpec;

pub use definitions::FetchDefinitionsExecutor;
pub use etymology::{FetchEtymologyExecutor, SegmentMorphologyExecutor};
pub use examples::FetchExamplesExecutor;
pub use extract_tokens::ExtractTokensExecutor;
pub use images::{FetchImagesExecutor, GenImageSdxlExecutor};
pub use tts::SynthTtsExecutor;

#[async_trait]
pub trait Executor: Send + Sync {
    fn kind(&self) -> JobKind;
    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError>;
}

/// Turn a store failure into a task failure.
///
/// Everything the store can fail with at this layer — a busy writer, a closed
/// channel during shutdown — is transient. A genuine constraint violation shows
/// up as `Conflict` and is a bug worth retrying loudly rather than silently
/// dropping the fetched content.
pub(crate) fn store_error(err: morpho_store::StoreError) -> TaskError {
    match err {
        morpho_store::StoreError::Invalid(what) => TaskError::permanent(what),
        other => TaskError::transient(other.to_string()),
    }
}

/// The full executor set.
pub fn default_executors(context: Arc<EngineContext>) -> Vec<Arc<dyn Executor>> {
    vec![
        Arc::new(ExtractTokensExecutor::new(context.pipeline.clone())),
        Arc::new(FetchDefinitionsExecutor::new(context.clone())),
        Arc::new(FetchExamplesExecutor::new(context.clone())),
        Arc::new(FetchEtymologyExecutor::new(context.clone())),
        Arc::new(SegmentMorphologyExecutor::new(context.clone())),
        Arc::new(FetchImagesExecutor::new(context.clone())),
        Arc::new(GenImageSdxlExecutor::new(context.clone())),
        Arc::new(SynthTtsExecutor::new(context)),
    ]
}

/// Payload mismatch: a rule and its executor disagree, which is a build-time
/// bug rather than anything a retry could fix.
pub(crate) fn wrong_payload(kind: JobKind) -> TaskError {
    TaskError::permanent(format!("{kind} job carried the wrong payload"))
}
