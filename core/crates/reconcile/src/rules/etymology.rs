//! Etymology: Wiktionary first, Morfessor segmentation as the fallback.
//!
//! Morfessor is the one batched job in the system. Adapter-protocol.md ruling
//! #4 says the ad-hoc trainer needs at least ~300 distinct words to produce
//! anything but nonsense, so the rule accumulates words whose Wiktionary lookup
//! is exhausted and only fires when either
//!
//! * the pending set has reached the configured batch size, or
//! * the oldest pending word has been waiting longer than the batch's maximum
//!   age — at which point an unsegmented answer beats an indefinite wait.
//!
//! The wait is derived from data (`source_fetch.fetched_at`, `job_state.updated_at`),
//! not from a timer in memory, so a restart does not reset the clock.

use std::sync::Arc;

use morpho_domain::job::{JobKey, JobKind, Priority, RateKey, SubjectRef};
use morpho_domain::time::parse_ts;
use morpho_store::error::Result;

use crate::engine::EngineContext;
use crate::facts::{FetchKind, SOURCE_MORFESSOR, SOURCE_WIKTIONARY};
use crate::rule::{JobPayload, JobSpec, Rule, Snapshot};

/// Subject id of the single Morfessor batch job.
pub const MORFESSOR_SUBJECT: &str = "morfessor_batch";

/// The adapter refuses batches larger than this (`adapters/morfessor` MAX_BATCH).
pub const MAX_ADAPTER_BATCH: usize = 5_000;

pub struct FetchEtymologyRule {
    context: Arc<EngineContext>,
}

impl FetchEtymologyRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for FetchEtymologyRule {
    fn name(&self) -> &'static str {
        "fetch_etymology"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        let _ = &self.context;
        let facts = snapshot.facts;
        let mut jobs = Vec::new();
        for word in &facts.active {
            if word.etymology.is_some() {
                continue;
            }
            if facts.fetched(&FetchKind::Etymology, word.word_id, SOURCE_WIKTIONARY) {
                continue;
            }
            jobs.push(
                JobSpec::new(
                    JobKey::new(
                        JobKind::FetchEtymology,
                        SubjectRef::word_source(word.word_id, SOURCE_WIKTIONARY),
                    ),
                    RateKey::Wiktionary,
                    Priority::P2,
                )
                .with_tiebreak(word.frequency_rank, word.word_id)
                .with_payload(JobPayload::FetchEtymology {
                    word_id: word.word_id,
                    lemma: word.lemma.clone(),
                }),
            );
        }
        Ok(jobs)
    }
}

pub struct SegmentMorphologyRule {
    context: Arc<EngineContext>,
}

impl SegmentMorphologyRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for SegmentMorphologyRule {
    fn name(&self) -> &'static str {
        "segment_morphology"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        let facts = snapshot.facts;
        let adapters = &self.context.sources.adapters;

        let mut pending: Vec<(i64, String)> = Vec::new();
        let mut oldest: Option<chrono::DateTime<chrono::Utc>> = None;

        for word in &facts.active {
            if word.etymology.is_some() {
                continue;
            }
            if facts.fetched(&FetchKind::Etymology, word.word_id, SOURCE_MORFESSOR) {
                continue;
            }
            if !facts.source_exhausted(
                &FetchKind::Etymology,
                JobKind::FetchEtymology,
                word.word_id,
                SOURCE_WIKTIONARY,
            ) {
                continue;
            }
            if let Some(at) = facts
                .exhausted_at(
                    &FetchKind::Etymology,
                    JobKind::FetchEtymology,
                    word.word_id,
                    SOURCE_WIKTIONARY,
                )
                .as_deref()
                .and_then(parse_ts)
            {
                oldest = Some(oldest.map_or(at, |current| current.min(at)));
            }
            pending.push((word.word_id, word.lemma.clone()));
        }

        if pending.is_empty() {
            return Ok(Vec::new());
        }

        let full_batch = pending.len() >= adapters.morfessor_batch;
        let aged = oldest.is_some_and(|at| {
            snapshot
                .now
                .signed_duration_since(at)
                .to_std()
                .unwrap_or_default()
                > adapters.morfessor_batch_max_age()
        });
        if !full_batch && !aged {
            tracing::debug!(
                pending = pending.len(),
                needed = adapters.morfessor_batch,
                "holding the morfessor batch until it fills or ages out"
            );
            return Ok(Vec::new());
        }

        // Deterministic membership: the fact set is already ordered by
        // (frequency_rank, word_id), so the same state always sends the same
        // batch — which matters because the ad-hoc model's version digest
        // covers the training words. A bigger batch is strictly better for the
        // trainer, so everything pending goes, up to the adapter's own ceiling.
        pending.truncate(MAX_ADAPTER_BATCH);

        Ok(vec![JobSpec::new(
            JobKey::new(
                JobKind::SegmentMorphology,
                SubjectRef::global(MORFESSOR_SUBJECT),
            ),
            RateKey::Cpu,
            // P3: an expensive generative fallback, behind everything that
            // could still be satisfied properly.
            Priority::P3,
        )
        .with_tiebreak(None, 0)
        .with_payload(JobPayload::SegmentMorphology {
            words: pending,
        })])
    }
}
