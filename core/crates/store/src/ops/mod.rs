//! Typed write operations.
//!
//! Every [`WriteOp`] is exactly one SQLite transaction, applied by the single
//! writer task. Audit-log rows are appended *inside* that transaction by the
//! operation itself, so a state change and its `events` row can never be
//! observed apart. [`WriteOp::Batch`] composes several operations into one
//! transaction for callers that need a larger atomic unit.

use rusqlite::Transaction;

use morpho_domain::change::{ChangeSet, EntityType};
use morpho_domain::event::{Actor, EventDraft};
use morpho_domain::job::{JobKey, JobStatus, RateKey};
use morpho_domain::types::{
    AuxStatus, CandidateKind, CreatedBy, DefinitionSource, EtymologySource, ExtractedToken,
    MediaKind, Role, SelectedBy, SlotRef, WordImport,
};

use crate::error::Result;

mod candidates;
mod derived;
mod distractors;
mod jobs;
mod oov;
mod plan;
mod readiness;
mod release;
mod selections;
mod tts;
mod words;

pub use candidates::{
    IngestDefinitions, IngestExamples, IngestImages, MediaRegistration, MintExampleCandidate,
    MintImageCandidate, FETCH_DEFINITIONS, FETCH_ETYMOLOGY, FETCH_EXAMPLES, FETCH_IMAGES,
};
pub use derived::{MarkMediaGc, RecordDefExtraction};
pub use distractors::{BindDistractors, DistractorBinding};
pub use jobs::UpsertJobState;
pub use oov::{OovResolution, SyncOosQueue};
pub use plan::{PlanGroupRow, PlanWordRow, WritePlan};
pub use readiness::{ApplyReadiness, ReadinessRow};
pub use release::RecordRelease;
pub use selections::{
    ApplyAutoSelections, ApplyScores, AutoSelection, MintDefinitionCandidate, ScoreUpdate,
    SetApproval, SetSelection,
};
pub use tts::RecordTtsAsset;
pub use words::{CreateWord, ImportStats, ImportWords, SetAuxStatus, SetEtymology, SetGloss};

/// One atomic unit of change.
#[derive(Debug, Clone)]
pub enum WriteOp {
    /// Bulk word-list import (idempotent: re-importing the same list is a no-op).
    ImportWords(ImportWords),
    /// Create a single word row (admin action / OOV promotion).
    CreateWord(CreateWord),
    /// Insert an immutable definition candidate, optionally selecting it.
    MintDefinitionCandidate(MintDefinitionCandidate),
    /// Insert an immutable example candidate.
    MintExampleCandidate(MintExampleCandidate),
    /// Insert an immutable image candidate (bytes already in the library).
    MintImageCandidate(MintImageCandidate),
    /// One definition fetch's whole result plus its completion marker.
    IngestDefinitions(IngestDefinitions),
    /// One example fetch's whole result plus its completion marker.
    IngestExamples(IngestExamples),
    /// One image fetch's whole result plus its completion marker.
    IngestImages(IngestImages),
    /// Point a selection slot at a candidate.
    SetSelection(SetSelection),
    /// Apply automatic selection decisions computed from a read snapshot.
    ApplyAutoSelections(ApplyAutoSelections),
    /// Store candidate scores.
    ApplyScores(ApplyScores),
    /// Approve or un-approve the current content of a slot.
    SetApproval(SetApproval),
    /// Move `is_primary` to one part of speech.
    SetPrimarySense { word_id: i64, pos: String },
    /// Enable or disable one sense slot.
    SetSlotEnabled {
        word_id: i64,
        pos: String,
        enabled: bool,
    },
    /// Mark a candidate rejected (and release the slot's pin if it was selected).
    RejectCandidate { kind: CandidateKind, cand_id: i64 },
    /// Reconcile `oos_queue` against the `oos_occurrences` view.
    SyncOosQueue(SyncOosQueue),
    /// Resolve one out-of-scope lemma by promotion or rewrite.
    ResolveOov {
        lemma: String,
        resolution: OovResolution,
    },
    /// Store a definition tokenization together with its input hash.
    RecordDefExtraction(RecordDefExtraction),
    /// Record a fetch completion marker (zero results included).
    RecordSourceFetch {
        kind: String,
        word_id: i64,
        source: String,
        result_count: i64,
    },
    /// Write a word's etymology.
    SetEtymology(SetEtymology),
    /// Set or clear a word's Chinese gloss anchor.
    SetGloss(SetGloss),
    /// Flip an auxiliary word between active and retired.
    SetAuxStatus(SetAuxStatus),
    /// Register a file in the content-addressed media registry.
    RegisterMediaFile {
        file_hash: String,
        kind: MediaKind,
        rel_path: String,
        bytes: i64,
    },
    /// Record one TTS synthesis (or its failure) with its media file.
    RecordTtsAsset(RecordTtsAsset),
    /// Bind missing distractors. Existing bindings are never rewritten.
    BindDistractors(BindDistractors),
    /// Publish a freshly computed learning plan.
    WritePlan(WritePlan),
    /// Refresh the `ready` / `core_ready` / `blockers` caches.
    ApplyReadiness(ApplyReadiness),
    /// Stamp or clear `media_files.gc_eligible_at`.
    MarkMediaGc(MarkMediaGc),
    /// Record a finished export and pin its media.
    RecordRelease(RecordRelease),
    /// Persist a failure state that must survive a restart.
    UpsertJobState(UpsertJobState),
    /// Drop a `job_state` row: success, or an operator hitting "retry".
    ClearJobState { key: JobKey },
    /// Append a bare audit-log row.
    AppendEvent {
        entity_type: EntityType,
        entity_id: String,
        action: morpho_domain::event::Action,
        detail: Option<serde_json::Value>,
    },
    /// Apply several operations in a single transaction.
    Batch(Vec<WriteOp>),
}

/// Payload returned by an applied [`WriteOp`].
#[derive(Debug, Clone)]
pub enum WriteResult {
    Unit,
    Import(ImportStats),
    Word {
        word_id: i64,
        created: bool,
    },
    DefinitionCandidate {
        def_cand_id: i64,
        created: bool,
    },
    /// Example or image candidate.
    Candidate {
        cand_id: i64,
        created: bool,
    },
    /// One bulk fetch ingest.
    Ingest {
        received: usize,
        created: usize,
    },
    /// `applied = false` means the computed input drifted before the write
    /// landed and the result was discarded (optimistic concurrency).
    Extraction {
        applied: bool,
    },
    Selected {
        applied: usize,
        skipped: usize,
    },
    Scored {
        changed: usize,
    },
    OosSync {
        opened: usize,
        closed: usize,
    },
    Bound {
        bound: usize,
    },
    Plan {
        plan_id: i64,
        applied: bool,
    },
    Readiness {
        changed: usize,
    },
    MediaGc {
        marked: usize,
        unmarked: usize,
    },
    Release {
        release_id: i64,
        created: bool,
    },
    Event {
        event_id: i64,
    },
    Batch(Vec<WriteResult>),
}

impl WriteResult {
    pub fn word_id(&self) -> Option<i64> {
        match self {
            Self::Word { word_id, .. } => Some(*word_id),
            _ => None,
        }
    }

    pub fn def_cand_id(&self) -> Option<i64> {
        match self {
            Self::DefinitionCandidate { def_cand_id, .. } => Some(*def_cand_id),
            _ => None,
        }
    }

    pub fn cand_id(&self) -> Option<i64> {
        match self {
            Self::Candidate { cand_id, .. } => Some(*cand_id),
            Self::DefinitionCandidate { def_cand_id, .. } => Some(*def_cand_id),
            _ => None,
        }
    }

    pub fn plan_id(&self) -> Option<i64> {
        match self {
            Self::Plan { plan_id, .. } => Some(*plan_id),
            _ => None,
        }
    }

    pub fn release_id(&self) -> Option<i64> {
        match self {
            Self::Release { release_id, .. } => Some(*release_id),
            _ => None,
        }
    }

    pub fn import_stats(&self) -> Option<&ImportStats> {
        match self {
            Self::Import(stats) => Some(stats),
            _ => None,
        }
    }
}

/// Transaction-scoped context handed to every operation.
pub(crate) struct OpCtx<'a, 'conn> {
    pub tx: &'a Transaction<'conn>,
    pub actor: &'a Actor,
    pub changes: &'a mut ChangeSet,
    pub now: String,
    pub events_written: usize,
}

impl OpCtx<'_, '_> {
    /// Append one audit-log row inside the current transaction.
    pub(crate) fn event(&mut self, draft: EventDraft) -> Result<i64> {
        let detail = match &draft.detail {
            Some(value) => Some(serde_json::to_string(value)?),
            None => None,
        };
        self.tx.execute(
            "INSERT INTO events (ts, actor, entity_type, entity_id, action, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                self.now,
                self.actor.to_string(),
                draft.entity_type.as_str(),
                draft.entity_id,
                draft.action.as_str(),
                detail
            ],
        )?;
        self.events_written += 1;
        Ok(self.tx.last_insert_rowid())
    }

    pub(crate) fn touch(&mut self, entity_type: EntityType, id: impl Into<String>) {
        self.changes.touch(entity_type, id);
    }
}

pub(crate) fn apply_op(op: WriteOp, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    match op {
        WriteOp::ImportWords(req) => words::import_words(req, ctx),
        WriteOp::CreateWord(req) => words::create_word(req, ctx),
        WriteOp::MintDefinitionCandidate(req) => selections::mint_definition_candidate(req, ctx),
        WriteOp::MintExampleCandidate(req) => candidates::mint_example_candidate(req, ctx),
        WriteOp::MintImageCandidate(req) => candidates::mint_image_candidate(req, ctx),
        WriteOp::IngestDefinitions(req) => candidates::ingest_definitions(req, ctx),
        WriteOp::IngestExamples(req) => candidates::ingest_examples(req, ctx),
        WriteOp::IngestImages(req) => candidates::ingest_images(req, ctx),
        WriteOp::SetSelection(req) => selections::set_selection(req, ctx),
        WriteOp::ApplyAutoSelections(req) => selections::apply_auto_selections(req, ctx),
        WriteOp::ApplyScores(req) => selections::apply_scores(req, ctx),
        WriteOp::SetApproval(req) => selections::set_approval(req, ctx),
        WriteOp::SetPrimarySense { word_id, pos } => {
            selections::set_primary_sense(word_id, &pos, ctx)
        }
        WriteOp::SetSlotEnabled {
            word_id,
            pos,
            enabled,
        } => selections::set_slot_enabled(word_id, &pos, enabled, ctx),
        WriteOp::RejectCandidate { kind, cand_id } => {
            selections::reject_candidate(kind, cand_id, ctx)
        }
        WriteOp::SyncOosQueue(req) => oov::sync_oos_queue(req, ctx),
        WriteOp::ResolveOov { lemma, resolution } => oov::resolve_oov(&lemma, resolution, ctx),
        WriteOp::RecordDefExtraction(req) => derived::record_def_extraction(req, ctx),
        WriteOp::RecordSourceFetch {
            kind,
            word_id,
            source,
            result_count,
        } => derived::record_source_fetch(&kind, word_id, &source, result_count, ctx),
        WriteOp::SetEtymology(req) => words::set_etymology(req, ctx),
        WriteOp::SetGloss(req) => words::set_gloss(req, ctx),
        WriteOp::SetAuxStatus(req) => words::set_aux_status(req, ctx),
        WriteOp::RegisterMediaFile {
            file_hash,
            kind,
            rel_path,
            bytes,
        } => derived::register_media_file(&file_hash, kind, &rel_path, bytes, ctx),
        WriteOp::RecordTtsAsset(req) => tts::record_tts_asset(req, ctx),
        WriteOp::BindDistractors(req) => distractors::bind_distractors(req, ctx),
        WriteOp::WritePlan(req) => plan::write_plan(req, ctx),
        WriteOp::ApplyReadiness(req) => readiness::apply_readiness(req, ctx),
        WriteOp::MarkMediaGc(req) => derived::mark_media_gc(req, ctx),
        WriteOp::RecordRelease(req) => release::record_release(req, ctx),
        WriteOp::UpsertJobState(req) => jobs::upsert_job_state(req, ctx),
        WriteOp::ClearJobState { key } => jobs::clear_job_state(&key, ctx),
        WriteOp::AppendEvent {
            entity_type,
            entity_id,
            action,
            detail,
        } => {
            let mut draft = EventDraft::new(entity_type, entity_id.clone(), action);
            draft.detail = detail;
            let event_id = ctx.event(draft)?;
            ctx.touch(entity_type, entity_id);
            Ok(WriteResult::Event { event_id })
        }
        WriteOp::Batch(ops) => {
            let mut results = Vec::with_capacity(ops.len());
            for op in ops {
                results.push(apply_op(op, ctx)?);
            }
            Ok(WriteResult::Batch(results))
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience constructors — keep call sites at the API/CLI edge readable.
// ---------------------------------------------------------------------------

impl WriteOp {
    pub fn import_words(role: Role, created_by: CreatedBy, words: Vec<WordImport>) -> Self {
        Self::ImportWords(ImportWords {
            role,
            created_by,
            words,
        })
    }

    pub fn mint_definition(
        word_id: i64,
        pos: impl Into<String>,
        text: impl Into<String>,
        source: DefinitionSource,
    ) -> Self {
        Self::MintDefinitionCandidate(MintDefinitionCandidate {
            word_id,
            pos: pos.into(),
            text: text.into(),
            source,
            source_ref: None,
            parent_cand_id: None,
            created_by: None,
            select: false,
        })
    }

    pub fn select(slot: SlotRef, cand_id: i64, selected_by: SelectedBy) -> Self {
        Self::SetSelection(SetSelection {
            pinned: selected_by == SelectedBy::Human,
            slot,
            cand_id,
            selected_by,
        })
    }

    pub fn approve(slot: SlotRef) -> Self {
        Self::SetApproval(SetApproval {
            slot,
            approved: true,
        })
    }

    pub fn unapprove(slot: SlotRef) -> Self {
        Self::SetApproval(SetApproval {
            slot,
            approved: false,
        })
    }

    pub fn set_etymology(word_id: i64, etymology: Option<String>, source: EtymologySource) -> Self {
        Self::SetEtymology(SetEtymology {
            word_id,
            etymology,
            source,
        })
    }

    pub fn set_gloss(word_id: i64, zh_gloss: impl Into<String>) -> Self {
        Self::SetGloss(SetGloss {
            word_id,
            zh_gloss: Some(zh_gloss.into()),
            source: morpho_domain::types::GlossSource::Manual,
        })
    }

    pub fn clear_gloss(word_id: i64) -> Self {
        Self::SetGloss(SetGloss {
            word_id,
            zh_gloss: None,
            source: morpho_domain::types::GlossSource::Manual,
        })
    }

    pub fn retire_aux(word_id: i64, reason: &'static str) -> Self {
        Self::SetAuxStatus(SetAuxStatus {
            word_id,
            status: AuxStatus::Retired,
            reason,
        })
    }

    pub fn reactivate_aux(word_id: i64, reason: &'static str) -> Self {
        Self::SetAuxStatus(SetAuxStatus {
            word_id,
            status: AuxStatus::Active,
            reason,
        })
    }

    pub fn record_extraction(
        def_cand_id: i64,
        expected_text_hash: impl Into<String>,
        input_hash: impl Into<String>,
        tokenizer_ver: impl Into<String>,
        lemmatizer_ver: impl Into<String>,
        tokens: Vec<ExtractedToken>,
    ) -> Self {
        Self::RecordDefExtraction(RecordDefExtraction {
            def_cand_id,
            expected_text_hash: expected_text_hash.into(),
            input_hash: input_hash.into(),
            tokenizer_ver: tokenizer_ver.into(),
            lemmatizer_ver: lemmatizer_ver.into(),
            tokens,
        })
    }

    pub fn job_backoff(
        key: JobKey,
        rate_key: RateKey,
        attempts: i64,
        next_retry_at: String,
        last_error: String,
    ) -> Self {
        Self::UpsertJobState(UpsertJobState {
            key,
            rate_key,
            status: JobStatus::Backoff,
            attempts,
            next_retry_at: Some(next_retry_at),
            last_error: Some(last_error),
        })
    }
}
