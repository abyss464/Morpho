//! `job_state` writes.
//!
//! The queue itself is never persisted; this table only remembers failure
//! states that must survive a restart. A successful run **deletes** the row —
//! no row means healthy (README Part 4 §"任务生命周期").

use rusqlite::OptionalExtension;

use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, EventDraft};
use morpho_domain::job::{JobKey, JobStatus, RateKey};

use super::{OpCtx, WriteResult};
use crate::error::Result;

/// Persist a backoff / dead / waived state for one job subject.
#[derive(Debug, Clone)]
pub struct UpsertJobState {
    pub key: JobKey,
    pub rate_key: RateKey,
    pub status: JobStatus,
    pub attempts: i64,
    pub next_retry_at: Option<String>,
    pub last_error: Option<String>,
}

fn entity_id(key: &JobKey) -> String {
    format!(
        "{}:{}:{}",
        key.kind, key.subject.subject_type, key.subject.subject_id
    )
}

pub(super) fn upsert_job_state(
    req: UpsertJobState,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    let previous: Option<String> = ctx
        .tx
        .query_row(
            "SELECT status FROM job_state
             WHERE kind = ?1 AND subject_type = ?2 AND subject_id = ?3",
            rusqlite::params![
                req.key.kind.as_str(),
                req.key.subject.subject_type.as_str(),
                req.key.subject.subject_id
            ],
            |row| row.get(0),
        )
        .optional()?;

    ctx.tx.execute(
        "INSERT INTO job_state
             (kind, subject_type, subject_id, rate_key, status, attempts,
              next_retry_at, last_error, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (kind, subject_type, subject_id) DO UPDATE SET
             rate_key = excluded.rate_key,
             status = excluded.status,
             attempts = excluded.attempts,
             next_retry_at = excluded.next_retry_at,
             last_error = excluded.last_error,
             updated_at = excluded.updated_at",
        rusqlite::params![
            req.key.kind.as_str(),
            req.key.subject.subject_type.as_str(),
            req.key.subject.subject_id,
            req.rate_key.as_str(),
            req.status.as_str(),
            req.attempts,
            req.next_retry_at,
            req.last_error,
            ctx.now,
        ],
    )?;

    // Only surface transitions a human cares about; per-attempt backoff churn
    // would flood the audit log.
    let previous_status = previous.as_deref();
    if previous_status != Some(req.status.as_str()) {
        let action = match req.status {
            JobStatus::Dead => Some(Action::JobDead),
            JobStatus::Waived => Some(Action::JobWaived),
            JobStatus::Backoff => None,
        };
        if let Some(action) = action {
            ctx.event(
                EventDraft::new(EntityType::JobState, entity_id(&req.key), action).detail(
                    serde_json::json!({
                        "kind": req.key.kind.as_str(),
                        "subject_type": req.key.subject.subject_type.as_str(),
                        "subject_id": req.key.subject.subject_id,
                        "attempts": req.attempts,
                        "last_error": req.last_error,
                    }),
                ),
            )?;
        }
    }

    ctx.touch(EntityType::JobState, entity_id(&req.key));
    Ok(WriteResult::Unit)
}

pub(super) fn clear_job_state(key: &JobKey, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let removed = ctx.tx.execute(
        "DELETE FROM job_state WHERE kind = ?1 AND subject_type = ?2 AND subject_id = ?3",
        rusqlite::params![
            key.kind.as_str(),
            key.subject.subject_type.as_str(),
            key.subject.subject_id
        ],
    )?;
    if removed > 0 {
        ctx.touch(EntityType::JobState, entity_id(key));
    }
    Ok(WriteResult::Unit)
}
