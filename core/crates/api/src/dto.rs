//! Response shapes for `docs/contracts/admin-api.md`.

use serde::{Deserialize, Serialize};

use morpho_domain::event::EventRecord;

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

#[derive(Debug, Default, Serialize)]
pub struct DashboardWords {
    pub total: i64,
    pub target: i64,
    pub auxiliary: i64,
    pub ready: i64,
    pub blocked: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct DashboardTts {
    pub ready: i64,
    pub missing: i64,
    pub failed: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct DashboardAssets {
    pub definitions: i64,
    pub examples: i64,
    pub images: i64,
    pub tts: DashboardTts,
}

#[derive(Debug, Serialize)]
pub struct DashboardPlan {
    pub plan_id: i64,
    pub built_at: String,
    pub group_count: i64,
}

#[derive(Debug, Serialize)]
pub struct Dashboard {
    pub words: DashboardWords,
    pub assets: DashboardAssets,
    pub oos_open: i64,
    pub dead_letters: i64,
    pub plan: Option<DashboardPlan>,
    pub recent_events: Vec<EventRecord>,
}

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

/// Row of `GET /words`.
#[derive(Debug, Serialize)]
pub struct WordRollup {
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

#[derive(Debug, Serialize)]
pub struct WordFields {
    pub word_id: i64,
    pub lemma: String,
    pub role: String,
    pub aux_status: Option<String>,
    pub phonetic: Option<String>,
    pub frequency_rank: Option<i64>,
    pub etymology: Option<String>,
    pub etymology_source: Option<String>,
    pub ready: bool,
    pub blockers: Vec<String>,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DefinitionCandidateDto {
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
    pub scorer_ver: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DefinitionSelectionDto {
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

/// One part-of-speech slot with its candidates.
#[derive(Debug, Serialize)]
pub struct DefinitionSlot {
    pub pos: String,
    pub selection: Option<DefinitionSelectionDto>,
    pub candidates: Vec<DefinitionCandidateDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExampleCandidateDto {
    pub ex_cand_id: i64,
    pub text: String,
    pub text_hash: String,
    pub hl_start: i64,
    pub hl_end: i64,
    pub source: String,
    pub status: String,
    pub auto_score: Option<f64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExampleSelectionDto {
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

#[derive(Debug, Serialize)]
pub struct ExampleBlock {
    pub slots: Vec<ExampleSelectionDto>,
    pub candidates: Vec<ExampleCandidateDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageCandidateDto {
    pub img_cand_id: i64,
    pub file_hash: String,
    pub pos: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub source: String,
    pub source_ref: Option<String>,
    pub license: Option<String>,
    pub status: String,
    pub auto_score: Option<f64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageSelectionDto {
    pub img_cand_id: i64,
    pub file_hash: String,
    pub selected_by: String,
    pub pinned: bool,
    pub approved: bool,
    pub approved_hash: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
    pub selection_rev: i64,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct ImageBlock {
    pub selection: Option<ImageSelectionDto>,
    pub candidates: Vec<ImageCandidateDto>,
}

/// TTS coverage of one text this word needs spoken.
#[derive(Debug, Serialize)]
pub struct TtsStatusDto {
    pub kind: String,
    pub text: String,
    /// `ready` | `failed` | `missing`
    pub status: String,
    pub file_hash: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DistractorDto {
    pub rank: i64,
    pub word_id: i64,
    pub lemma: String,
    pub core_ready: bool,
}

/// `GET /words/{id}`
#[derive(Debug, Serialize)]
pub struct WordDetail {
    #[serde(flatten)]
    pub word: WordFields,
    pub definitions: Vec<DefinitionSlot>,
    pub examples: ExampleBlock,
    pub image: ImageBlock,
    pub tts: Vec<TtsStatusDto>,
    pub distractors: Vec<DistractorDto>,
    pub recent_events: Vec<EventRecord>,
}

// ---------------------------------------------------------------------------
// Mutation request bodies
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MintDefinitionBody {
    pub word_id: i64,
    pub pos: String,
    pub text: String,
    #[serde(default)]
    pub parent_cand_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SelectionBody {
    pub word_id: i64,
    #[serde(default)]
    pub pos: Option<String>,
    #[serde(default)]
    pub slot: Option<i64>,
    pub cand_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct ApprovalBody {
    pub word_id: i64,
    #[serde(default)]
    pub pos: Option<String>,
    #[serde(default)]
    pub slot: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum OovResolveBody {
    Promote {
        #[serde(default)]
        phonetic: Option<String>,
        #[serde(default)]
        frequency_rank: Option<i64>,
    },
    Rewrite {
        def_cand_id: i64,
        text: String,
    },
}

/// `POST /oov/{lemma}/resolve`
#[derive(Debug, Serialize)]
pub struct OovResolveResponse {
    pub oos_lemma: String,
    pub status: String,
    pub word_id: Option<i64>,
    pub def_cand_id: Option<i64>,
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
    }
}
