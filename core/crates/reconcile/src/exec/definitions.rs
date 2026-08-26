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
//!
//! The Free Dictionary path carries a second product. Its payload attaches a
//! usage sentence to individual senses, so the examples are already in hand by
//! the time the definitions are parsed (admin-api.md ruling #18). Both go in
//! one `Batch` — one transaction, two completion markers — which is what makes
//! the sentences free: the alternative is a second request per word to the same
//! endpoint for bytes this executor already downloaded.

use std::sync::Arc;

use async_trait::async_trait;

use morpho_domain::error::{ErrorKind, TaskError};
use morpho_domain::event::Actor;
use morpho_domain::job::JobKind;
use morpho_domain::types::{DefinitionSource, ExampleSource, FetchedDefinition, FetchedExample};
use morpho_store::ops::{IngestDefinitions, IngestExamples};
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

        let (definitions, phonetic, examples) = match source {
            DefinitionSource::Freedict => {
                match freedict::fetch(
                    &self.context.sources.http,
                    &self.context.sources.config,
                    lemma,
                )
                .await
                {
                    Ok(entry) => (entry.definitions, entry.phonetic, Some(entry.examples)),
                    // A word this dictionary does not carry is a real answer —
                    // for both products, so both markers still get written.
                    Err(err) if err.kind() == ErrorKind::Permanent => {
                        tracing::debug!(lemma, error = %err, "freedict has no entry");
                        (Vec::new(), None, Some(Vec::new()))
                    }
                    Err(err) => return Err(err),
                }
            }
            DefinitionSource::Wordnet => {
                let Some(wordnet) = self.context.sources.wordnet.as_ref() else {
                    return Err(TaskError::permanent("WordNet is not configured"));
                };
                // WordNet glosses carry no usage sentences.
                (wordnet.definitions(lemma, WORDNET_SENSE_LIMIT), None, None)
            }
            other => {
                return Err(TaskError::permanent(format!(
                    "{other} is not a fetchable definition source"
                )))
            }
        };

        commit(store, *word_id, *source, definitions, phonetic, examples).await
    }
}

async fn commit(
    store: &Store,
    word_id: i64,
    source: DefinitionSource,
    definitions: Vec<FetchedDefinition>,
    phonetic: Option<String>,
    mined_examples: Option<Vec<FetchedExample>>,
) -> Result<(), TaskError> {
    let ingest = WriteOp::IngestDefinitions(IngestDefinitions {
        word_id,
        source,
        definitions,
        phonetic,
    });
    let op = match mined_examples {
        // One transaction: either both products and both markers land, or
        // neither does, and the next pass re-derives the whole fetch.
        Some(examples) => WriteOp::Batch(vec![
            ingest,
            WriteOp::IngestExamples(IngestExamples {
                word_id,
                source: ExampleSource::Freedict,
                examples,
            }),
        ]),
        None => ingest,
    };
    store
        .write(Actor::Worker(JobKind::FetchDefinitions), op)
        .await
        .map_err(store_error)?;
    Ok(())
}
