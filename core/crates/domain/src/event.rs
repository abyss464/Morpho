//! Append-only audit log vocabulary (`events` table).
//!
//! Every human-visible state transition writes one row **inside the same
//! transaction** as the change it describes, so "why did the engine do that"
//! is always reconstructible (README Part 3).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::change::EntityType;
use crate::job::JobKind;

/// `events.actor`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// The reconciler loop itself.
    Reconciler,
    /// A job executor.
    Worker(JobKind),
    /// An admin API caller.
    Admin(String),
    /// A `morphod` subcommand run from a terminal.
    Cli,
    /// Test fixtures and internal maintenance.
    System(String),
}

impl Actor {
    pub fn admin(user: impl Into<String>) -> Self {
        Self::Admin(user.into())
    }
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reconciler => f.write_str("reconciler"),
            Self::Worker(kind) => write!(f, "worker:{kind}"),
            Self::Admin(user) => write!(f, "admin:{user}"),
            Self::Cli => f.write_str("cli"),
            Self::System(name) => write!(f, "system:{name}"),
        }
    }
}

/// `events.action`. Kept as a closed enum so the admin UI can rely on the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    WordsImported,
    WordCreated,
    CandidateAdded,
    CandidateRejected,
    SelectionChanged,
    Approved,
    Unapproved,
    ApprovalInvalidated,
    PinFallback,
    PrimaryMoved,
    SlotEnabledChanged,
    AuxPromoted,
    AuxRetired,
    OosResolved,
    OosOpened,
    OosAutoClosed,
    PlanRebuilt,
    DistractorBound,
    JobDead,
    JobWaived,
    JobRetried,
    SourceFetched,
    EtymologySet,
    GlossSet,
    GlossCleared,
    MediaGcMarked,
    ReleaseExported,
}

impl Action {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WordsImported => "words_imported",
            Self::WordCreated => "word_created",
            Self::CandidateAdded => "candidate_added",
            Self::CandidateRejected => "candidate_rejected",
            Self::SelectionChanged => "selection_changed",
            Self::Approved => "approved",
            Self::Unapproved => "unapproved",
            Self::ApprovalInvalidated => "approval_invalidated",
            Self::PinFallback => "pin_fallback",
            Self::PrimaryMoved => "primary_moved",
            Self::SlotEnabledChanged => "slot_enabled_changed",
            Self::AuxPromoted => "aux_promoted",
            Self::AuxRetired => "aux_retired",
            Self::OosResolved => "oos_resolved",
            Self::OosOpened => "oos_opened",
            Self::OosAutoClosed => "oos_auto_closed",
            Self::PlanRebuilt => "plan_rebuilt",
            Self::DistractorBound => "distractor_bound",
            Self::JobDead => "job_dead",
            Self::JobWaived => "job_waived",
            Self::JobRetried => "job_retried",
            Self::SourceFetched => "source_fetched",
            Self::EtymologySet => "etymology_set",
            Self::GlossSet => "gloss_set",
            Self::GlossCleared => "gloss_cleared",
            Self::MediaGcMarked => "media_gc_marked",
            Self::ReleaseExported => "release_exported",
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A row of the audit log as returned by `GET /api/events`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub event_id: i64,
    pub ts: String,
    pub actor: String,
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    /// Parsed `detail` JSON, or `null` when the column was NULL / unparsable.
    pub detail: serde_json::Value,
}

/// What a write operation wants appended to the audit log.
#[derive(Debug, Clone)]
pub struct EventDraft {
    pub entity_type: EntityType,
    pub entity_id: String,
    pub action: Action,
    pub detail: Option<serde_json::Value>,
}

impl EventDraft {
    pub fn new(entity_type: EntityType, entity_id: impl Into<String>, action: Action) -> Self {
        Self {
            entity_type,
            entity_id: entity_id.into(),
            action,
            detail: None,
        }
    }

    #[must_use]
    pub fn detail(mut self, detail: serde_json::Value) -> Self {
        self.detail = Some(detail);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_strings_match_the_contract() {
        assert_eq!(Actor::Reconciler.to_string(), "reconciler");
        assert_eq!(
            Actor::Worker(JobKind::ExtractTokens).to_string(),
            "worker:extract_tokens"
        );
        assert_eq!(Actor::admin("abyss").to_string(), "admin:abyss");
        assert_eq!(Actor::Cli.to_string(), "cli");
    }

    #[test]
    fn action_serde_matches_display() {
        for action in [Action::CandidateAdded, Action::ApprovalInvalidated] {
            let json = serde_json::to_string(&action).unwrap();
            assert_eq!(json, format!("\"{}\"", action.as_str()));
        }
    }
}
