//! Wire shapes for `docs/contracts/admin-api.md`.
//!
//! `admin-ui/src/api/types.ts` is normative (wave-2 ruling #1): every struct
//! here mirrors one interface there, field for field. Two consequences run
//! through the whole file:
//!
//! * SQLite's `INTEGER 0/1` becomes a real JSON boolean;
//! * TEXT columns that hold JSON (`blockers`, `score_detail`, `stats_json`,
//!   `events.detail`) are decoded server-side into objects, not passed through
//!   as strings.

use serde::{Deserialize, Serialize};

/// `{"items": [...], "total": n}`
#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// `?page=1&page_size=50`
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Pagination {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

impl Pagination {
    pub const DEFAULT_PAGE_SIZE: i64 = 50;
    pub const MAX_PAGE_SIZE: i64 = 200;

    pub fn limit(&self) -> i64 {
        self.page_size
            .unwrap_or(Self::DEFAULT_PAGE_SIZE)
            .clamp(1, Self::MAX_PAGE_SIZE)
    }

    pub fn offset(&self) -> i64 {
        (self.page.unwrap_or(1).max(1) - 1) * self.limit()
    }
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: Some(1),
            page_size: Some(Self::DEFAULT_PAGE_SIZE),
        }
    }
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

/// `AssetRollup`
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct AssetRollup {
    pub ready: i64,
    pub missing: i64,
    pub failed: i64,
}

/// `DashboardWordStats`
#[derive(Debug, Default, Serialize)]
pub struct DashboardWords {
    pub total: i64,
    pub target: i64,
    /// Active auxiliaries only (wave-2 ruling #8).
    pub auxiliary: i64,
    pub ready: i64,
    pub blocked: i64,
}

/// `DashboardAssets`
#[derive(Debug, Default, Serialize)]
pub struct DashboardAssets {
    pub definitions: AssetRollup,
    pub examples: AssetRollup,
    pub images: AssetRollup,
    pub tts: AssetRollup,
}

/// `DashboardPlan`
#[derive(Debug, Serialize)]
pub struct DashboardPlan {
    pub plan_id: i64,
    pub built_at: String,
    pub group_count: i64,
}

/// `DashboardResponse`
#[derive(Debug, Serialize)]
pub struct Dashboard {
    pub words: DashboardWords,
    pub assets: DashboardAssets,
    pub oos_open: i64,
    pub dead_letters: i64,
    pub plan: Option<DashboardPlan>,
    pub recent_events: Vec<morpho_domain::event::EventRecord>,
}

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

/// `WordListItem`
#[derive(Debug, Serialize)]
pub struct WordListItem {
    pub word_id: i64,
    pub lemma: String,
    pub role: String,
    pub ready: bool,
    pub blockers: Vec<String>,
    pub has_image: bool,
    pub sense_count: i64,
    pub example_count: i64,
    pub tts_missing: i64,
}

/// `Word`
#[derive(Debug, Clone, Serialize)]
pub struct Word {
    pub word_id: i64,
    pub lemma: String,
    pub role: String,
    pub aux_status: Option<String>,
    pub phonetic: Option<String>,
    pub frequency_rank: Option<i64>,
    pub etymology: Option<String>,
    pub etymology_source: Option<String>,
    /// Non-null makes this word a gloss anchor: it takes no assets, no plan
    /// slot and no readiness verdict, and every dependency on it is satisfied
    /// (admin-api.md ruling #18a).
    pub zh_gloss: Option<String>,
    pub zh_gloss_source: Option<String>,
    pub ready: bool,
    pub blockers: Vec<String>,
    pub created_by: String,
    pub created_at: String,
}

/// `DefinitionCandidate`
#[derive(Debug, Clone, Serialize)]
pub struct DefinitionCandidate {
    pub def_cand_id: i64,
    pub word_id: i64,
    pub pos: String,
    pub text: String,
    pub text_hash: String,
    pub source: String,
    pub source_ref: Option<String>,
    pub parent_cand_id: Option<i64>,
    pub status: String,
    pub auto_score: Option<f64>,
    pub score_detail: Option<serde_json::Value>,
    pub scorer_ver: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

/// `ExampleCandidate`
#[derive(Debug, Clone, Serialize)]
pub struct ExampleCandidate {
    pub ex_cand_id: i64,
    pub word_id: i64,
    pub text: String,
    pub text_hash: String,
    pub hl_start: i64,
    pub hl_end: i64,
    pub source: String,
    pub source_ref: Option<String>,
    pub status: String,
    pub auto_score: Option<f64>,
    pub score_detail: Option<serde_json::Value>,
    pub scorer_ver: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

/// `ImageCandidate`
#[derive(Debug, Clone, Serialize)]
pub struct ImageCandidate {
    pub img_cand_id: i64,
    pub word_id: i64,
    pub pos: Option<String>,
    pub file_hash: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub source: String,
    pub source_ref: Option<String>,
    pub license: Option<String>,
    pub query_used: Option<String>,
    pub status: String,
    pub auto_score: Option<f64>,
    pub score_detail: Option<serde_json::Value>,
    pub scorer_ver: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

/// `DefinitionSelection`
#[derive(Debug, Clone, Serialize)]
pub struct DefinitionSelection {
    pub word_id: i64,
    pub pos: String,
    pub def_cand_id: i64,
    pub is_primary: bool,
    pub enabled: bool,
    pub selected_by: String,
    pub pinned: bool,
    pub approved: bool,
    pub approved_hash: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
    pub selection_rev: i64,
    pub updated_at: String,
}

/// `ExampleSelection`
#[derive(Debug, Clone, Serialize)]
pub struct ExampleSelection {
    pub word_id: i64,
    pub slot: i64,
    pub ex_cand_id: i64,
    pub selected_by: String,
    pub pinned: bool,
    pub approved: bool,
    pub approved_hash: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
    pub selection_rev: i64,
    pub updated_at: String,
}

/// `ImageSelection`
#[derive(Debug, Clone, Serialize)]
pub struct ImageSelection {
    pub word_id: i64,
    pub img_cand_id: i64,
    pub selected_by: String,
    pub pinned: bool,
    pub approved: bool,
    pub approved_hash: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
    pub selection_rev: i64,
    pub updated_at: String,
}

/// `DefinitionSlotView`
#[derive(Debug, Serialize)]
pub struct DefinitionSlotView {
    pub pos: String,
    pub selection: Option<DefinitionSelection>,
    pub candidates: Vec<DefinitionCandidate>,
}

/// `ExampleSlotView`
#[derive(Debug, Serialize)]
pub struct ExampleSlotView {
    pub slot: i64,
    pub selection: Option<ExampleSelection>,
    pub candidates: Vec<ExampleCandidate>,
}

/// `ImageSlotView`
#[derive(Debug, Serialize)]
pub struct ImageSlotView {
    pub selection: Option<ImageSelection>,
    pub candidates: Vec<ImageCandidate>,
}

/// Which selected slot a TTS text belongs to.
#[derive(Debug, Clone, Serialize)]
pub struct TtsRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<i64>,
}

/// `TtsStatusView`
#[derive(Debug, Clone, Serialize)]
pub struct TtsStatusView {
    pub kind: String,
    pub text: String,
    pub text_hash: String,
    pub input_hash: String,
    pub voice: String,
    pub engine: String,
    pub engine_ver: String,
    /// `ready | failed | missing` (wave-2 ruling #6).
    pub status: String,
    pub file_hash: Option<String>,
    pub duration_ms: Option<i64>,
    pub r#ref: Option<TtsRef>,
    pub last_error: Option<String>,
}

/// `DistractorView`
#[derive(Debug, Clone, Serialize)]
pub struct DistractorView {
    pub rank: i64,
    pub word_id: i64,
    pub lemma: String,
    pub core_ready: bool,
    pub blockers: Vec<String>,
    pub bound_at: String,
    pub bound_by: String,
}

/// `WordDetail`
#[derive(Debug, Serialize)]
pub struct WordDetail {
    pub word: Word,
    pub definitions: Vec<DefinitionSlotView>,
    /// Always three entries, slots 1..3.
    pub examples: Vec<ExampleSlotView>,
    pub image: ImageSlotView,
    pub tts: Vec<TtsStatusView>,
    pub distractors: Vec<DistractorView>,
    pub recent_events: Vec<morpho_domain::event::EventRecord>,
}

// ---------------------------------------------------------------------------
// OOV queue
// ---------------------------------------------------------------------------

/// `OovOccurrence`
#[derive(Debug, Clone, Serialize)]
pub struct OovOccurrence {
    pub word_id: i64,
    pub lemma: String,
    pub pos: String,
    pub def_cand_id: i64,
    pub text: String,
    pub hits: i64,
    /// Newest available `llm_rewrite` candidate for this definition, else null
    /// (wave-2 ruling #10).
    pub suggested_rewrite: Option<String>,
}

/// `OovQueueEntry`
#[derive(Debug, Serialize)]
pub struct OovQueueEntry {
    pub oos_lemma: String,
    pub status: String,
    pub first_seen: String,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<String>,
    pub notes: Option<String>,
    pub occurrences: Vec<OovOccurrence>,
    pub occurrence_count: usize,
}

// ---------------------------------------------------------------------------
// Dead letters
// ---------------------------------------------------------------------------

/// `DeadLetter["subject"]`
#[derive(Debug, Clone, Serialize)]
pub struct DeadLetterSubject {
    pub word_id: Option<i64>,
    pub lemma: Option<String>,
    pub label: String,
}

/// `DeadLetter`
#[derive(Debug, Serialize)]
pub struct DeadLetter {
    pub kind: String,
    pub subject_type: String,
    pub subject_id: String,
    pub rate_key: String,
    pub status: String,
    pub attempts: i64,
    pub next_retry_at: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
    pub subject: DeadLetterSubject,
}

/// `JobKeyBody`
#[derive(Debug, Deserialize)]
pub struct JobKeyBody {
    pub kind: String,
    pub subject_type: String,
    pub subject_id: String,
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

/// `PlanStats`
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct PlanStats {
    pub word_count: i64,
    pub group_count: i64,
    pub edge_count: i64,
    pub scc_group_count: i64,
    pub largest_group: i64,
    pub avg_group_size: f64,
}

/// `PlanGroupSummary`
#[derive(Debug, Serialize)]
pub struct PlanGroupSummary {
    pub group_seq: i64,
    pub group_type: String,
    pub word_count: i64,
    pub ready_count: i64,
    pub first_lemma: String,
    pub last_lemma: String,
}

/// `PlanDiff`
#[derive(Debug, Default, Serialize)]
pub struct PlanDiff {
    pub previous_plan_id: Option<i64>,
    pub added: i64,
    pub removed: i64,
    pub reordered: i64,
}

/// `PlanSummary`
#[derive(Debug, Serialize)]
pub struct PlanSummary {
    pub plan_id: i64,
    pub input_hash: String,
    pub algo_ver: String,
    pub params: serde_json::Value,
    pub is_current: bool,
    pub built_at: String,
    pub stats: PlanStats,
    pub groups: Vec<PlanGroupSummary>,
    pub diff: PlanDiff,
}

/// `PlanWordView`
#[derive(Debug, Serialize)]
pub struct PlanWordView {
    pub word_id: i64,
    pub lemma: String,
    pub role: String,
    pub learning_order: i64,
    pub group_seq: i64,
    pub ready: bool,
    pub blockers: Vec<String>,
}

/// `PlanGroupDetail`
#[derive(Debug, Serialize)]
pub struct PlanGroupDetail {
    pub plan_id: i64,
    pub group_seq: i64,
    pub group_type: String,
    pub words: Vec<PlanWordView>,
}

// ---------------------------------------------------------------------------
// Releases
// ---------------------------------------------------------------------------

/// `Release`
#[derive(Debug, Serialize)]
pub struct Release {
    pub release_id: i64,
    pub version: String,
    pub plan_id: i64,
    pub input_hash: String,
    pub db_file_hash: String,
    pub exported_at: String,
    pub exported_by: String,
    pub notes: Option<String>,
    pub word_count: i64,
    pub media_count: i64,
    pub total_bytes: i64,
}

/// `ExportBody`
#[derive(Debug, Default, Deserialize)]
pub struct ExportBody {
    #[serde(default)]
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// Mutation request bodies
// ---------------------------------------------------------------------------

/// `CreateWordBody`
#[derive(Debug, Deserialize)]
pub struct CreateWordBody {
    pub lemma: String,
    pub role: String,
    #[serde(default)]
    pub phonetic: Option<String>,
    #[serde(default)]
    pub frequency_rank: Option<i64>,
}

/// `MintDefinitionBody`
#[derive(Debug, Deserialize)]
pub struct MintDefinitionBody {
    pub word_id: i64,
    pub pos: String,
    pub text: String,
    #[serde(default)]
    pub parent_cand_id: Option<i64>,
}

/// `MintExampleBody`
#[derive(Debug, Deserialize)]
pub struct MintExampleBody {
    pub word_id: i64,
    pub text: String,
    pub hl_start: i64,
    pub hl_end: i64,
}

/// `OverrideSelectionBody`
#[derive(Debug, Deserialize)]
pub struct SelectionBody {
    pub word_id: i64,
    #[serde(default)]
    pub pos: Option<String>,
    #[serde(default)]
    pub slot: Option<i64>,
    pub cand_id: i64,
}

/// `SelectionKeyBody`
#[derive(Debug, Deserialize)]
pub struct ApprovalBody {
    pub word_id: i64,
    #[serde(default)]
    pub pos: Option<String>,
    #[serde(default)]
    pub slot: Option<i64>,
}

/// `SetPrimaryBody`
#[derive(Debug, Deserialize)]
pub struct SetPrimaryBody {
    pub word_id: i64,
    pub pos: String,
}

/// `SetEnabledBody`
#[derive(Debug, Deserialize)]
pub struct SetEnabledBody {
    pub word_id: i64,
    pub pos: String,
    pub enabled: bool,
}

/// `SetGlossBody`
#[derive(Debug, Deserialize)]
pub struct SetGlossBody {
    pub zh_gloss: String,
}

/// `OovResolveBody`
#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum OovResolveBody {
    Promote {
        #[serde(default)]
        notes: Option<String>,
        #[serde(default)]
        phonetic: Option<String>,
        #[serde(default)]
        frequency_rank: Option<i64>,
    },
    Rewrite {
        def_cand_id: i64,
        text: String,
        #[serde(default)]
        notes: Option<String>,
    },
    /// Ruling #18a: ground the lemma in Chinese instead of teaching it or
    /// writing it out of the definition.
    Gloss {
        zh_gloss: String,
        #[serde(default)]
        notes: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Query filters
// ---------------------------------------------------------------------------

// NOTE: query structs deliberately avoid `#[serde(flatten)]` — the query-string
// deserializer cannot type non-string values through a flattened map.

#[derive(Debug, Default, Deserialize)]
pub struct WordListQuery {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub ready: Option<bool>,
    #[serde(default)]
    pub blocker: Option<String>,
    #[serde(default)]
    pub group: Option<i64>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

impl WordListQuery {
    pub fn pagination(&self) -> Pagination {
        Pagination {
            page: self.page,
            page_size: self.page_size,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct EventQuery {
    #[serde(default)]
    pub entity_type: Option<String>,
    #[serde(default)]
    pub entity_id: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

impl EventQuery {
    pub fn pagination(&self) -> Pagination {
        Pagination {
            page: self.page,
            page_size: self.page_size,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct OovQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

impl OovQuery {
    pub fn pagination(&self) -> Pagination {
        Pagination {
            page: self.page,
            page_size: self.page_size,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

impl PageQuery {
    pub fn pagination(&self) -> Pagination {
        Pagination {
            page: self.page,
            page_size: self.page_size,
        }
    }
}

// ---------------------------------------------------------------------------
// Gallery
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct GalleryItem {
    pub word_id: i64,
    pub lemma: String,
    pub role: String,
    pub img_cand_id: i64,
    pub file_hash: String,
    pub source: String,
    pub auto_score: Option<f64>,
    pub approved: bool,
    pub selected_by: String,
    pub pinned: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct GalleryQuery {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub approved: Option<bool>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

impl GalleryQuery {
    pub fn pagination(&self) -> Pagination {
        Pagination {
            page: self.page,
            page_size: self.page_size,
        }
    }
}

/// `GET /dead-letters?rate_key=&page=&page_size=` (wave-3 ruling #14).
#[derive(Debug, Default, Deserialize)]
pub struct DeadLetterQuery {
    /// Dispatcher lane to narrow to. Anything unrecognized matches nothing
    /// rather than failing the request.
    #[serde(default)]
    pub rate_key: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

impl DeadLetterQuery {
    pub fn pagination(&self) -> Pagination {
        Pagination {
            page: self.page,
            page_size: self.page_size,
        }
    }
}

/// Decode a TEXT column holding JSON into a value the console can read.
pub fn decode_json(raw: Option<String>) -> Option<serde_json::Value> {
    raw.as_deref()
        .and_then(|text| serde_json::from_str(text).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_defaults_and_clamps() {
        let p = Pagination {
            page: None,
            page_size: None,
        };
        assert_eq!(p.limit(), 50);
        assert_eq!(p.offset(), 0);

        let p = Pagination {
            page: Some(3),
            page_size: Some(20),
        };
        assert_eq!(p.offset(), 40);

        let p = Pagination {
            page: Some(0),
            page_size: Some(10_000),
        };
        assert_eq!(p.limit(), Pagination::MAX_PAGE_SIZE);
        assert_eq!(p.offset(), 0);
    }

    #[test]
    fn oov_body_matches_the_contract() {
        let promote: OovResolveBody = serde_json::from_str(r#"{"mode":"promote"}"#).unwrap();
        assert!(matches!(promote, OovResolveBody::Promote { .. }));
        let rewrite: OovResolveBody =
            serde_json::from_str(r#"{"mode":"rewrite","def_cand_id":3,"text":"kind"}"#).unwrap();
        assert!(matches!(
            rewrite,
            OovResolveBody::Rewrite { def_cand_id: 3, .. }
        ));
        // `notes` rides along on both arms.
        let with_notes: OovResolveBody =
            serde_json::from_str(r#"{"mode":"promote","notes":"seen in 3 defs"}"#).unwrap();
        assert!(matches!(
            with_notes,
            OovResolveBody::Promote { notes: Some(_), .. }
        ));
    }

    #[test]
    fn text_json_columns_are_decoded_not_passed_through() {
        let decoded = decode_json(Some(r#"{"total":0.8}"#.to_string())).unwrap();
        assert!(decoded.is_object());
        assert_eq!(decoded["total"], 0.8);
        assert!(decode_json(None).is_none());
        assert!(decode_json(Some("not json".into())).is_none());
    }

    #[test]
    fn tts_ref_omits_absent_keys() {
        let word = TtsRef {
            pos: None,
            slot: None,
        };
        assert_eq!(serde_json::to_string(&word).unwrap(), "{}");
        let sense = TtsRef {
            pos: Some("adj".into()),
            slot: None,
        };
        assert_eq!(serde_json::to_string(&sense).unwrap(), r#"{"pos":"adj"}"#);
    }

    #[test]
    fn the_tts_ref_field_serializes_without_the_raw_marker() {
        let view = TtsStatusView {
            kind: "word".into(),
            text: "serene".into(),
            text_hash: "t".into(),
            input_hash: "i".into(),
            voice: "v".into(),
            engine: "edge-tts".into(),
            engine_ver: "7".into(),
            status: "missing".into(),
            file_hash: None,
            duration_ms: None,
            r#ref: None,
            last_error: None,
        };
        let json = serde_json::to_string(&view).unwrap();
        assert!(json.contains(r#""ref":null"#), "{json}");
        assert!(!json.contains("r#ref"));
    }
}
