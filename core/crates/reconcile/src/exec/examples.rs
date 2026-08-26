//! Example fetching from the exam corpus, the Free Dictionary and Tatoeba.
//!
//! Whatever the source, the shape is the same one every fetch executor uses:
//! one atomic `IngestExamples` carrying every sentence the source produced
//! *and* its completion marker, so a legitimately empty answer is terminal
//! rather than re-derived forever (README Part 4 §"完成标记").
//!
//! Highlight offsets are always computed against the canonicalized sentence,
//! which is the same text the store will hold, by the shared
//! [`sentence`](crate::sources::sentence) miner — the corpus indexes them at
//! load time, the two HTTP sources at parse time, and neither gets its own
//! subtly different idea of where a word starts.

use std::sync::Arc;

use async_trait::async_trait;

use morpho_domain::error::{ErrorKind, TaskError};
use morpho_domain::event::Actor;
use morpho_domain::job::JobKind;
use morpho_domain::types::{ExampleSource, FetchedExample};
use morpho_store::ops::IngestExamples;
use morpho_store::{Store, WriteOp};

use crate::engine::EngineContext;
use crate::exec::{store_error, wrong_payload, Executor};
use crate::rule::{JobPayload, JobSpec};
use crate::sources::{freedict, tatoeba};

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
        let JobPayload::FetchExamples {
            word_id,
            lemma,
            source,
        } = &job.payload
        else {
            return Err(wrong_payload(JobKind::FetchExamples));
        };

        let examples = match source {
            ExampleSource::ExamCorpus => {
                let Some(corpus) = self.context.sources.corpus.as_ref() else {
                    return Err(TaskError::permanent("the exam corpus is not configured"));
                };
                // Loaded once at startup, so this path does no I/O at all.
                corpus.examples(lemma).to_vec()
            }
            // The backfill path: this word's definitions were fetched before
            // the payload was being mined, so the payload is fetched again for
            // its usage sentences alone. Fresh words never take this route —
            // `FetchDefinitionsExecutor` commits both products at once.
            ExampleSource::Freedict => {
                match freedict::fetch(
                    &self.context.sources.http,
                    &self.context.sources.config,
                    lemma,
                )
                .await
                {
                    Ok(entry) => entry.examples,
                    // A word this dictionary does not carry is a real answer.
                    Err(err) if err.kind() == ErrorKind::Permanent => {
                        tracing::debug!(lemma, error = %err, "freedict has no entry to mine");
                        Vec::new()
                    }
                    Err(err) => return Err(err),
                }
            }
            ExampleSource::Tatoeba => {
                match tatoeba::search(
                    &self.context.sources.http,
                    &self.context.sources.config,
                    lemma,
                )
                .await
                {
                    Ok(examples) => examples,
                    Err(err) if err.kind() == ErrorKind::Permanent => {
                        tracing::debug!(lemma, error = %err, "tatoeba has no sentences");
                        Vec::new()
                    }
                    Err(err) => return Err(err),
                }
            }
            other => {
                return Err(TaskError::permanent(format!(
                    "{other} is not a fetchable example source"
                )))
            }
        };

        commit(store, *word_id, *source, examples).await
    }
}

/// One fetch's whole result plus its completion marker, in one transaction.
pub(crate) async fn commit(
    store: &Store,
    word_id: i64,
    source: ExampleSource,
    examples: Vec<FetchedExample>,
) -> Result<(), TaskError> {
    store
        .write(
            Actor::Worker(JobKind::FetchExamples),
            WriteOp::IngestExamples(IngestExamples {
                word_id,
                source,
                examples,
            }),
        )
        .await
        .map_err(store_error)?;
    Ok(())
}
