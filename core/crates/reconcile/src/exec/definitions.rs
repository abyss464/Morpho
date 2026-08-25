//! Definition fetching against the Free Dictionary API and WordNet.
//!
//! Both paths end the same way: one `IngestDefinitions` write carrying every
//! candidate the source produced *and* the completion marker. A word the source
//! genuinely has nothing for still gets its marker, with `result_count = 0`,
//! which is what stops the rule re-deriving forever and what lets the WordNet
//! fallback know the Free Dictionary is spent.
//!
//! A 404 arrives as `Permanent`; the executor turns it into the same
//! zero-result ingest rather than a dead letter, because "this dictionary does
//! not have this word" is an answer, not a failure.

use std::sync::Arc;

use async_trait::async_trait;

use morpho_domain::error::{ErrorKind, TaskError};
use morpho_domain::event::Actor;
use morpho_domain::job::JobKind;
use morpho_domain::types::{DefinitionSource, FetchedDefinition};
use morpho_store::ops::IngestDefinitions;
use morpho_store::{Store, WriteOp};

use crate::engine::EngineContext;
use crate::exec::{store_error, wrong_payload, Executor};
use crate::rule::{JobPayload, JobSpec};
use crate::sources::freedict;

/// WordNet senses turned into candidates. Beyond a handful the glosses get
/// obscure enough that they are noise for a learner.
const WORDNET_SENSE_LIMIT: usize = 4;

pub struct FetchDefinitionsExecutor {
    context: Arc<EngineContext>,
}

impl FetchDefinitionsExecutor {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Executor for FetchDefinitionsExecutor {
    fn kind(&self) -> JobKind {
        JobKind::FetchDefinitions
    }

    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError> {
        let JobPayload::FetchDefinitions {
            word_id,
            lemma,
            source,
        } = &job.payload
        else {
            return Err(wrong_payload(JobKind::FetchDefinitions));
        };

        let (definitions, phonetic) = match source {
            DefinitionSource::Freedict => {
                match freedict::fetch(
                    &self.context.sources.http,
                    &self.context.sources.config,
                    lemma,
                )
                .await
                {
                    Ok(entry) => (entry.definitions, entry.phonetic),
                    // A word this dictionary does not carry is a real answer.
                    Err(err) if err.kind() == ErrorKind::Permanent => {
                        tracing::debug!(lemma, error = %err, "freedict has no entry");
                        (Vec::new(), None)
                    }
                    Err(err) => return Err(err),
                }
            }
            DefinitionSource::Wordnet => {
                let Some(wordnet) = self.context.sources.wordnet.as_ref() else {
                    return Err(TaskError::permanent("WordNet is not configured"));
                };
                (wordnet.definitions(lemma, WORDNET_SENSE_LIMIT), None)
            }
            other => {
                return Err(TaskError::permanent(format!(
                    "{other} is not a fetchable definition source"
                )))
            }
        };

        commit(store, *word_id, *source, definitions, phonetic).await
    }
}

async fn commit(
    store: &Store,
    word_id: i64,
    source: DefinitionSource,
    definitions: Vec<FetchedDefinition>,
    phonetic: Option<String>,
) -> Result<(), TaskError> {
    store
        .write(
            Actor::Worker(JobKind::FetchDefinitions),
            WriteOp::IngestDefinitions(IngestDefinitions {
                word_id,
                source,
                definitions,
                phonetic,
            }),
        )
        .await
        .map_err(store_error)?;
    Ok(())
}
