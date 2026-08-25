//! Etymology from Wiktionary, and the Morfessor segmentation fallback.

use std::sync::Arc;

use async_trait::async_trait;

use morpho_domain::error::{ErrorKind, TaskError};
use morpho_domain::event::Actor;
use morpho_domain::job::JobKind;
use morpho_domain::types::EtymologySource;
use morpho_store::ops::{SetEtymology, FETCH_ETYMOLOGY};
use morpho_store::{Store, WriteOp};

use crate::engine::EngineContext;
use crate::exec::{store_error, wrong_payload, Executor};
use crate::facts::{SOURCE_MORFESSOR, SOURCE_WIKTIONARY};
use crate::rule::{JobPayload, JobSpec};
use crate::sources::{proc, wiktionary};

pub struct FetchEtymologyExecutor {
    context: Arc<EngineContext>,
}

impl FetchEtymologyExecutor {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Executor for FetchEtymologyExecutor {
    fn kind(&self) -> JobKind {
        JobKind::FetchEtymology
    }

    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError> {
        let JobPayload::FetchEtymology { word_id, lemma } = &job.payload else {
            return Err(wrong_payload(JobKind::FetchEtymology));
        };

        let etymology = match wiktionary::fetch(
            &self.context.sources.http,
            &self.context.sources.config,
            lemma,
        )
        .await
        {
            Ok(value) => value,
            // A page Wiktionary does not have is an answer, not a fault: mark
            // the source spent so Morfessor can take over.
            Err(err) if err.kind() == ErrorKind::Permanent => {
                tracing::debug!(lemma, error = %err, "wiktionary has no etymology");
                None
            }
            Err(err) => return Err(err),
        };

        let found = usize::from(etymology.is_some());
        let mut ops = vec![WriteOp::RecordSourceFetch {
            kind: FETCH_ETYMOLOGY.to_string(),
            word_id: *word_id,
            source: SOURCE_WIKTIONARY.to_string(),
            result_count: found as i64,
        }];
        if etymology.is_some() {
            ops.push(WriteOp::SetEtymology(SetEtymology {
                word_id: *word_id,
                etymology,
                source: EtymologySource::Wiktionary,
            }));
        }

        store
            .write(Actor::Worker(JobKind::FetchEtymology), WriteOp::Batch(ops))
            .await
            .map_err(store_error)?;
        Ok(())
    }
}

pub struct SegmentMorphologyExecutor {
    context: Arc<EngineContext>,
}

impl SegmentMorphologyExecutor {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Executor for SegmentMorphologyExecutor {
    fn kind(&self) -> JobKind {
        JobKind::SegmentMorphology
    }

    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError> {
        let JobPayload::SegmentMorphology { words } = &job.payload else {
            return Err(wrong_payload(JobKind::SegmentMorphology));
        };
        if words.is_empty() {
            return Ok(());
        }

        let lemmas: Vec<String> = words.iter().map(|(_, lemma)| lemma.clone()).collect();
        let result = proc::morfessor_segment(&self.context.sources.adapters, &lemmas).await?;

        let mut ops = Vec::with_capacity(words.len() * 2);
        let mut segmented = 0usize;
        for (word_id, lemma) in words {
            // The adapter keys its answer on the exact string it was handed.
            let morphs = result.segments.get(lemma).cloned().unwrap_or_default();
            // A single morph is the adapter saying "no structure here" — that
            // is a real answer, but it is not an etymology, so it is recorded
            // as a zero-result fetch rather than written into the column.
            let useful = morphs.len() > 1;
            if useful {
                ops.push(WriteOp::SetEtymology(SetEtymology {
                    word_id: *word_id,
                    etymology: Some(morphs.join(" + ")),
                    source: EtymologySource::Morfessor,
                }));
                segmented += 1;
            }
            ops.push(WriteOp::RecordSourceFetch {
                kind: FETCH_ETYMOLOGY.to_string(),
                word_id: *word_id,
                source: SOURCE_MORFESSOR.to_string(),
                result_count: i64::from(useful),
            });
        }

        tracing::info!(
            batch = words.len(),
            segmented,
            model_ver = %result.model_ver,
            "morfessor batch complete"
        );

        store
            .write(
                Actor::Worker(JobKind::SegmentMorphology),
                WriteOp::Batch(ops),
            )
            .await
            .map_err(store_error)?;
        Ok(())
    }
}
