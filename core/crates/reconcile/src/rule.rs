//! The desired-state rule interface.
//!
//! A rule is a pure function from a read snapshot to the set of jobs the world
//! would need in order to match the desired state. Rules never write, never
//! deduplicate against in-flight work, and never consult backoff state — the
//! reconciler does all of that centrally, which is what keeps "full pass" and
//! "partial pass" provably equivalent modulo latency.
//!
//! Rules derive **external** work only. Everything local — scoring, automatic
//! selection, OOV sync, auxiliary liveness, distractor binding, plan rebuild,
//! readiness, media GC — runs inline as an ordered maintenance sweep in
//! [`crate::stages`], because those stages read the results of the previous one
//! and dispatching them as independent jobs would only add a round trip per
//! stage per pass.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use morpho_domain::change::{ChangeEvent, EntityType};
use morpho_domain::job::{JobKey, Priority, RateKey};
use morpho_domain::tts::DesiredTts;
use morpho_domain::types::{DefinitionSource, ExampleSource, ImageSource};
use morpho_store::error::Result;

use crate::facts::Facts;

/// What a pass is allowed to look at.
///
/// `Partial` is purely an optimization: a full derivation always yields a
/// superset of any partial one (README Part 4 §"对账循环").
#[derive(Debug, Clone, Default)]
pub enum Scope {
    #[default]
    Full,
    Partial(Vec<ChangeEvent>),
}

impl Scope {
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full)
    }

    /// Ids of one entity type touched by this scope, or `None` for a full pass.
    pub fn touched(&self, entity_type: EntityType) -> Option<BTreeSet<&str>> {
        match self {
            Self::Full => None,
            Self::Partial(events) => Some(
                events
                    .iter()
                    .filter(|e| e.entity_type == entity_type)
                    .flat_map(|e| e.entity_ids.iter().map(String::as_str))
                    .collect(),
            ),
        }
    }
}

/// A read-only view of the world handed to every rule during one pass.
///
/// `facts` covers what every rule needs; `conn` is the same read connection the
/// facts came from, for the rare rule whose input is too big to preload.
pub struct Snapshot<'a> {
    pub conn: &'a rusqlite::Connection,
    pub facts: &'a Facts,
    pub scope: &'a Scope,
    pub now: DateTime<Utc>,
}

/// Per-kind job input, carried from derivation to execution so the executor
/// does not have to re-read what the rule already saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobPayload {
    /// No extra input; the executor loads what it needs.
    None,
    ExtractTokens {
        def_cand_id: i64,
        text: String,
        text_hash: String,
    },
    FetchDefinitions {
        word_id: i64,
        lemma: String,
        source: DefinitionSource,
    },
    FetchEtymology {
        word_id: i64,
        lemma: String,
    },
    /// One Morfessor batch (adapter-protocol.md ruling #4).
    SegmentMorphology {
        words: Vec<(i64, String)>,
    },
    FetchExamples {
        word_id: i64,
        lemma: String,
        source: ExampleSource,
    },
    FetchImages {
        word_id: i64,
        lemma: String,
        source: ImageSource,
        /// Selected primary gloss, used to make the search query specific.
        gloss: Option<String>,
    },
    GenImageSdxl {
        word_id: i64,
        lemma: String,
        gloss: Option<String>,
    },
    SynthTts {
        desired: DesiredTts,
    },
}

/// One unit of work the world is missing.
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub key: JobKey,
    pub rate_key: RateKey,
    pub priority: Priority,
    /// Deterministic tiebreak within a priority band: `(frequency_rank, id)`.
    /// Missing ranks sort last (README: ties break by frequency_rank then id).
    pub tiebreak: (i64, i64),
    pub payload: JobPayload,
    /// `job_state.attempts` at derivation time, if a row existed. Filled in by
    /// the reconciler, not by rules.
    pub prior_attempts: Option<i64>,
}

impl JobSpec {
    pub fn new(key: JobKey, rate_key: RateKey, priority: Priority) -> Self {
        Self {
            key,
            rate_key,
            priority,
            tiebreak: (i64::MAX, 0),
            payload: JobPayload::None,
            prior_attempts: None,
        }
    }

    #[must_use]
    pub fn with_tiebreak(mut self, frequency_rank: Option<i64>, id: i64) -> Self {
        self.tiebreak = (frequency_rank.unwrap_or(i64::MAX), id);
        self
    }

    #[must_use]
    pub fn with_payload(mut self, payload: JobPayload) -> Self {
        self.payload = payload;
        self
    }

    /// Total order used by the dispatcher: priority band, then frequency, then
    /// id, then the key itself. Fully deterministic.
    pub fn ordering_key(&self) -> (Priority, i64, i64, &JobKey) {
        (self.priority, self.tiebreak.0, self.tiebreak.1, &self.key)
    }
}

/// A desired-state rule.
pub trait Rule: Send + Sync {
    /// Stable name, used in logs and metrics.
    fn name(&self) -> &'static str;

    /// Derive the jobs this rule wants, given a read snapshot.
    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use morpho_domain::job::{JobKind, SubjectRef};

    #[test]
    fn ordering_puts_p0_and_common_words_first() {
        let mut specs = [
            JobSpec::new(
                JobKey::new(JobKind::SynthTts, SubjectRef::word(3)),
                RateKey::EdgeTts,
                Priority::P2,
            )
            .with_tiebreak(Some(10), 3),
            JobSpec::new(
                JobKey::new(JobKind::ExtractTokens, SubjectRef::def_candidate(9)),
                RateKey::Cpu,
                Priority::P0,
            )
            .with_tiebreak(Some(4000), 9),
            JobSpec::new(
                JobKey::new(JobKind::ExtractTokens, SubjectRef::def_candidate(2)),
                RateKey::Cpu,
                Priority::P0,
            )
            .with_tiebreak(None, 2),
        ];
        specs.sort_by(|a, b| a.ordering_key().cmp(&b.ordering_key()));
        assert_eq!(specs[0].key.subject.subject_id, "9");
        assert_eq!(specs[1].key.subject.subject_id, "2");
        assert_eq!(specs[2].key.kind, JobKind::SynthTts);
    }

    #[test]
    fn partial_scope_reports_touched_ids() {
        let scope = Scope::Partial(vec![ChangeEvent {
            entity_type: EntityType::DefinitionCandidate,
            entity_ids: vec!["1".into(), "2".into()],
        }]);
        let touched = scope.touched(EntityType::DefinitionCandidate).unwrap();
        assert!(touched.contains("1"));
        assert!(scope.touched(EntityType::Word).unwrap().is_empty());
        assert!(Scope::Full.touched(EntityType::Word).is_none());
    }
}
