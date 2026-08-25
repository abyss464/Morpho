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
    CandidateKind, CreatedBy, DefinitionSource, ExtractedToken, MediaKind, Role, SelectedBy,
    SlotRef, WordImport,
};

use crate::error::Result;

mod derived;
mod jobs;
mod oov;
mod selections;
mod words;

pub use derived::RecordDefExtraction;
pub use jobs::UpsertJobState;
pub use oov::OovResolution;
pub use selections::{MintDefinitionCandidate, SetApproval, SetSelection};
pub use words::{CreateWord, ImportStats, ImportWords};

/// One atomic unit of change.
#[derive(Debug, Clone)]
pub enum WriteOp {
    /// Bulk word-list import (idempotent: re-importing the same list is a no-op).
    ImportWords(ImportWords),
    /// Create a single word row (admin action / OOV promotion).
    CreateWord(CreateWord),
    /// Insert an immutable definition candidate, optionally selecting it.
    MintDefinitionCandidate(MintDefinitionCandidate),
    /// Point a selection slot at a candidate.
    SetSelection(SetSelection),
    /// Approve or un-approve the current content of a slot.
    SetApproval(SetApproval),
    /// Mark a candidate rejected (and release the slot's pin if it was selected).
    RejectCandidate { kind: CandidateKind, cand_id: i64 },
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
    /// Register a file in the content-addressed media registry.
    RegisterMediaFile {
        file_hash: String,
        kind: MediaKind,
        rel_path: String,
        bytes: i64,
    },
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
    /// `applied = false` means the computed input drifted before the write
    /// landed and the result was discarded (optimistic concurrency).
    Extraction {
        applied: bool,
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
        WriteOp::SetSelection(req) => selections::set_selection(req, ctx),
        WriteOp::SetApproval(req) => selections::set_approval(req, ctx),
        WriteOp::RejectCandidate { kind, cand_id } => {
            selections::reject_candidate(kind, cand_id, ctx)
        }
        WriteOp::ResolveOov { lemma, resolution } => oov::resolve_oov(&lemma, resolution, ctx),
        WriteOp::RecordDefExtraction(req) => derived::record_def_extraction(req, ctx),
        WriteOp::RecordSourceFetch {
            kind,
            word_id,
            source,
            result_count,
        } => derived::record_source_fetch(&kind, word_id, &source, result_count, ctx),
        WriteOp::RegisterMediaFile {
            file_hash,
            kind,
            rel_path,
            bytes,
        } => derived::register_media_file(&file_hash, kind, &rel_path, bytes, ctx),
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
