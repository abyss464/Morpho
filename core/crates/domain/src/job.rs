//! Job vocabulary: kinds, subjects, lanes, priorities and the in-memory queue
//! snapshot served by `GET /api/jobs`.
//!
//! The queue itself is derived, never stored (README Part 4). Only failure
//! state that must survive a restart lives in `job_state`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::types::ParseEnumError;

macro_rules! job_enum {
    ($(#[$meta:meta])* $name:ident, $kind:literal, { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = ParseEnumError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($text => Ok(Self::$variant),)+
                    other => Err(ParseEnumError { kind: $kind, value: other.to_string() }),
                }
            }
        }
    };
}

job_enum!(
    /// `job_state.kind`
    JobKind, "job_kind", {
        // Local CPU work.
        ExtractTokens => "extract_tokens",
        ScoreCandidates => "score_candidates",
        AutoSelect => "auto_select",
        SyncOosQueue => "sync_oos_queue",
        SyncAuxLiveness => "sync_aux_liveness",
        RecomputeReadiness => "recompute_readiness",
        BindDistractors => "bind_distractors",
        BuildPlan => "build_plan",
        GcMedia => "gc_media",
        // External fetches / generation.
        FetchDefinitions => "fetch_definitions",
        FetchExamples => "fetch_examples",
        FetchEtymology => "fetch_etymology",
        FetchImages => "fetch_images",
        SegmentMorphology => "segment_morphology",
        GenImageSdxl => "gen_image_sdxl",
        RewriteDefinition => "rewrite_definition",
        SynthTts => "synth_tts",
    }
);

job_enum!(
    /// `job_state.rate_key` — one dispatcher lane each.
    ///
    /// The set mirrors the normative `rate_limits` seed rows in
    /// `docs/contracts/working-db.sql`. WordNet and Morfessor are in-process /
    /// local-subprocess work and therefore share the `cpu` lane.
    RateKey, "rate_key", {
        Cpu => "cpu",
        Freedict => "freedict",
        Wiktionary => "wiktionary",
        Unsplash => "unsplash",
        Pexels => "pexels",
        Pixabay => "pixabay",
        Wikimedia => "wikimedia",
        Openverse => "openverse",
        Tatoeba => "tatoeba",
        Sdxl => "sdxl",
        EdgeTts => "edge_tts",
        Llm => "llm",
    }
);

job_enum!(
    /// `job_state.subject_type`
    SubjectType, "subject_type", {
        Word => "word",
        DefCandidate => "def_candidate",
        TtsInput => "tts_input",
        Global => "global",
    }
);

job_enum!(
    /// `job_state.status`
    JobStatus, "job_status", {
        Backoff => "backoff",
        Dead => "dead",
        Waived => "waived",
    }
);

impl JobKind {
    /// Lane this kind of work runs on. Kinds that fan out over several external
    /// sources (definitions, images) override it per job.
    pub const fn default_rate_key(self) -> RateKey {
        match self {
            Self::ExtractTokens
            | Self::ScoreCandidates
            | Self::AutoSelect
            | Self::SyncOosQueue
            | Self::SyncAuxLiveness
            | Self::RecomputeReadiness
            | Self::BindDistractors
            | Self::BuildPlan
            | Self::GcMedia
            | Self::SegmentMorphology
            | Self::FetchExamples => RateKey::Cpu,
            Self::FetchDefinitions => RateKey::Freedict,
            Self::FetchEtymology => RateKey::Wiktionary,
            Self::FetchImages => RateKey::Unsplash,
            Self::GenImageSdxl => RateKey::Sdxl,
            Self::RewriteDefinition => RateKey::Llm,
            Self::SynthTts => RateKey::EdgeTts,
        }
    }

    /// Attempt count after which a subject is declared dead
    /// (README Part 4: HTTP 8, TTS 5, SDXL 3, LLM 4).
    pub const fn dead_after_attempts(self) -> u32 {
        match self {
            Self::GenImageSdxl => 3,
            Self::RewriteDefinition => 4,
            Self::SynthTts => 5,
            Self::FetchDefinitions | Self::FetchEtymology | Self::FetchImages => 8,
            // Local CPU work: a repeated failure is a bug, surface it early.
            _ => 5,
        }
    }
}

/// Priority band. Lower value runs first (README Part 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Cheap local computation that unblocks everything else.
    P0,
    /// Regeneration invalidated by an edit; anything blocking a pending release.
    P1,
    /// Backlog backfill, ordered by learning order.
    P2,
    /// Expensive generative fallbacks.
    P3,
}

/// Identifies what a job is about; maps onto `job_state`'s composite key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SubjectRef {
    pub subject_type: SubjectType,
    pub subject_id: String,
}

impl SubjectRef {
    pub fn new(subject_type: SubjectType, subject_id: impl Into<String>) -> Self {
        Self {
            subject_type,
            subject_id: subject_id.into(),
        }
    }

    pub fn word(word_id: i64) -> Self {
        Self::new(SubjectType::Word, word_id.to_string())
    }

    /// A per-source fan-out of a word job: `"{word_id}:{source}"`.
    ///
    /// The composite key of `job_state` has no source column, so the source
    /// rides in `subject_id`. That is what lets one stock-photo provider die
    /// or be waived without silencing the other two.
    pub fn word_source(word_id: i64, source: &str) -> Self {
        Self::new(SubjectType::Word, format!("{word_id}:{source}"))
    }

    pub fn def_candidate(def_cand_id: i64) -> Self {
        Self::new(SubjectType::DefCandidate, def_cand_id.to_string())
    }

    pub fn tts_input(input_hash: impl Into<String>) -> Self {
        Self::new(SubjectType::TtsInput, input_hash)
    }

    pub fn global(name: impl Into<String>) -> Self {
        Self::new(SubjectType::Global, name)
    }

    /// Numeric word id encoded in this subject, if any.
    pub fn word_id(&self) -> Option<i64> {
        if self.subject_type != SubjectType::Word {
            return None;
        }
        let head = self
            .subject_id
            .split_once(':')
            .map_or(self.subject_id.as_str(), |(head, _)| head);
        head.parse().ok()
    }

    /// Source suffix of a `word_source` subject, if any.
    pub fn source(&self) -> Option<&str> {
        self.subject_id.split_once(':').map(|(_, tail)| tail)
    }
}

impl fmt::Display for SubjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.subject_type, self.subject_id)
    }
}

/// Primary key of `job_state`, and the in-flight dedup key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct JobKey {
    pub kind: JobKind,
    pub subject: SubjectRef,
}

impl JobKey {
    pub fn new(kind: JobKind, subject: SubjectRef) -> Self {
        Self { kind, subject }
    }
}

impl fmt::Display for JobKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.kind, self.subject)
    }
}

/// One row of `GET /api/jobs` (`JobView` in admin-ui/src/api/types.ts).
///
/// `status` is `null` for work that only exists in memory; `job_state` rows
/// carry `backoff | dead | waived`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobView {
    pub kind: String,
    pub subject_type: SubjectType,
    pub subject_id: String,
    pub rate_key: String,
    pub status: Option<JobStatus>,
    pub attempts: i64,
    pub next_retry_at: Option<String>,
    pub last_error: Option<String>,
    pub subject_label: Option<String>,
}

/// Per-lane counters of `GET /api/jobs` (`LaneView` in types.ts).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LaneView {
    pub queued: usize,
    pub running: usize,
    pub limit: usize,
}

/// Body of `GET /api/jobs` (`JobsSnapshot` in types.ts).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobsSnapshot {
    pub in_flight: Vec<JobView>,
    pub backoff: Vec<JobView>,
    pub lanes: std::collections::BTreeMap<String, LaneView>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_kind_strings_round_trip() {
        for kind in JobKind::ALL {
            assert_eq!(JobKind::from_str(kind.as_str()).unwrap(), *kind);
        }
        for key in RateKey::ALL {
            assert_eq!(RateKey::from_str(key.as_str()).unwrap(), *key);
        }
    }

    #[test]
    fn every_lane_has_a_contract_seed_row() {
        // docs/contracts/working-db.sql seeds exactly these rate keys.
        let seeded = [
            "freedict",
            "wiktionary",
            "unsplash",
            "pexels",
            "pixabay",
            "wikimedia",
            "openverse",
            "tatoeba",
            "sdxl",
            "edge_tts",
            "llm",
            "cpu",
        ];
        for key in RateKey::ALL {
            assert!(
                seeded.contains(&key.as_str()),
                "lane {key} has no rate_limits seed"
            );
        }
    }

    #[test]
    fn local_work_lands_on_the_cpu_lane() {
        assert_eq!(JobKind::ExtractTokens.default_rate_key(), RateKey::Cpu);
        assert_eq!(JobKind::SegmentMorphology.default_rate_key(), RateKey::Cpu);
        assert_eq!(JobKind::SynthTts.default_rate_key(), RateKey::EdgeTts);
    }

    #[test]
    fn dead_thresholds_match_the_whitepaper() {
        assert_eq!(JobKind::FetchDefinitions.dead_after_attempts(), 8);
        assert_eq!(JobKind::SynthTts.dead_after_attempts(), 5);
        assert_eq!(JobKind::GenImageSdxl.dead_after_attempts(), 3);
        assert_eq!(JobKind::RewriteDefinition.dead_after_attempts(), 4);
    }

    #[test]
    fn priority_orders_p0_first() {
        let mut v = vec![Priority::P3, Priority::P0, Priority::P2, Priority::P1];
        v.sort();
        assert_eq!(
            v,
            vec![Priority::P0, Priority::P1, Priority::P2, Priority::P3]
        );
    }

    #[test]
    fn job_key_display_is_stable() {
        let key = JobKey::new(JobKind::ExtractTokens, SubjectRef::def_candidate(42));
        assert_eq!(key.to_string(), "extract_tokens/def_candidate:42");
    }

    #[test]
    fn word_source_subjects_decode() {
        let subject = SubjectRef::word_source(7, "pexels");
        assert_eq!(subject.subject_id, "7:pexels");
        assert_eq!(subject.word_id(), Some(7));
        assert_eq!(subject.source(), Some("pexels"));

        let plain = SubjectRef::word(7);
        assert_eq!(plain.word_id(), Some(7));
        assert_eq!(plain.source(), None);

        let global = SubjectRef::global("morfessor");
        assert_eq!(global.word_id(), None);
    }
}
