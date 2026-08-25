//! Read-side helpers shared by the CLI, the reconciler and the admin API.
//!
//! Anything that is naturally a *view* is read from the view — those can never
//! be stale (README Part 3).

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use morpho_domain::job::{JobKey, JobKind, JobStatus, RateKey, SubjectRef, SubjectType};

use crate::error::Result;

/// Coarse counts used by `morphod status` and the dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WordCounts {
    pub total: i64,
    pub target: i64,
    pub base: i64,
    pub auxiliary: i64,
    pub auxiliary_active: i64,
    pub ready: i64,
    pub blocked: i64,
}

pub fn word_counts(conn: &Connection) -> Result<WordCounts> {
    let counts = conn.query_row(
        "SELECT
             COUNT(*),
             COALESCE(SUM(role = 'target'), 0),
             COALESCE(SUM(role = 'base'), 0),
             COALESCE(SUM(role = 'auxiliary'), 0),
             COALESCE(SUM(role = 'auxiliary' AND aux_status = 'active'), 0),
             COALESCE(SUM(ready = 1), 0)
         FROM words",
        [],
        |row| {
            Ok(WordCounts {
                total: row.get(0)?,
                target: row.get(1)?,
                base: row.get(2)?,
                auxiliary: row.get(3)?,
                auxiliary_active: row.get(4)?,
                ready: row.get(5)?,
                blocked: 0,
            })
        },
    )?;
    // "blocked" counts words that should be ready but are not: everything the
    // active_words view covers, minus the ready ones.
    let active: i64 = conn.query_row("SELECT COUNT(*) FROM active_words", [], |row| row.get(0))?;
    Ok(WordCounts {
        blocked: (active - counts.ready).max(0),
        ..counts
    })
}

/// Asset-side counts used by the dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetCounts {
    pub definition_candidates: i64,
    pub definitions: i64,
    pub example_candidates: i64,
    pub examples: i64,
    pub image_candidates: i64,
    pub images: i64,
    pub tts_ready: i64,
    pub tts_failed: i64,
    pub tts_missing: i64,
}

pub fn asset_counts(conn: &Connection) -> Result<AssetCounts> {
    let scalar = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |row| row.get(0))?) };
    Ok(AssetCounts {
        definition_candidates: scalar("SELECT COUNT(*) FROM definition_candidates")?,
        definitions: scalar("SELECT COUNT(*) FROM definition_selections WHERE enabled = 1")?,
        example_candidates: scalar("SELECT COUNT(*) FROM example_candidates")?,
        examples: scalar("SELECT COUNT(*) FROM example_selections")?,
        image_candidates: scalar("SELECT COUNT(*) FROM image_candidates")?,
        images: scalar("SELECT COUNT(*) FROM image_selections")?,
        tts_ready: scalar("SELECT COUNT(*) FROM tts_assets WHERE status = 'ready'")?,
        tts_failed: scalar("SELECT COUNT(*) FROM tts_assets WHERE status = 'failed'")?,
        // Desired minus covered. Voice/params are not yet configurable, so the
        // desired set is matched on (kind, text) alone.
        tts_missing: scalar(
            "SELECT COUNT(*) FROM tts_desired d
             LEFT JOIN tts_assets a
               ON a.kind = d.kind AND a.text = d.text AND a.status = 'ready'
             WHERE a.tts_id IS NULL",
        )?,
    })
}

/// Count of open out-of-scope queue rows.
pub fn oos_open_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM oos_queue WHERE status = 'open'",
        [],
        |row| row.get(0),
    )?)
}

/// Count of dead-lettered jobs.
pub fn dead_letter_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM job_state WHERE status = 'dead'",
        [],
        |row| row.get(0),
    )?)
}

/// One persisted job state row.
#[derive(Debug, Clone)]
pub struct JobStateRow {
    pub key: JobKey,
    pub rate_key: RateKey,
    pub status: JobStatus,
    pub attempts: i64,
    pub next_retry_at: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

/// Load every `job_state` row. The table only holds failures, so it stays
/// small enough to read wholesale each reconcile pass.
pub fn job_states(conn: &Connection) -> Result<Vec<JobStateRow>> {
    let mut stmt = conn.prepare(
        "SELECT kind, subject_type, subject_id, rate_key, status, attempts,
                next_retry_at, last_error, updated_at
         FROM job_state",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (kind, subject_type, subject_id, rate_key, status, attempts, next, err, updated) = row?;
        // Unknown vocabulary means a newer build wrote the row; skip rather
        // than fail the whole pass.
        let (Ok(kind), Ok(subject_type), Ok(rate_key), Ok(status)) = (
            kind.parse::<JobKind>(),
            subject_type.parse::<SubjectType>(),
            rate_key.parse::<RateKey>(),
            status.parse::<JobStatus>(),
        ) else {
            tracing::warn!(kind, subject_type, "skipping unrecognized job_state row");
            continue;
        };
        out.push(JobStateRow {
            key: JobKey::new(kind, SubjectRef::new(subject_type, subject_id)),
            rate_key,
            status,
            attempts,
            next_retry_at: next,
            last_error: err,
            updated_at: updated,
        });
    }
    Ok(out)
}

/// Lane limits as configured in the `rate_limits` table.
#[derive(Debug, Clone, Copy)]
pub struct RateLimitRow {
    pub rate_key: RateKey,
    pub max_concurrency: i64,
    pub refill_per_min: f64,
    pub burst: i64,
}

pub fn rate_limits(conn: &Connection) -> Result<Vec<RateLimitRow>> {
    let mut stmt =
        conn.prepare("SELECT rate_key, max_concurrency, refill_per_min, burst FROM rate_limits")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (key, max_concurrency, refill_per_min, burst) = row?;
        match key.parse::<RateKey>() {
            Ok(rate_key) => out.push(RateLimitRow {
                rate_key,
                max_concurrency,
                refill_per_min,
                burst,
            }),
            Err(_) => tracing::warn!(rate_key = %key, "ignoring unknown rate_limits row"),
        }
    }
    Ok(out)
}

/// Resolve a lemma to its word id (case-insensitive, per the schema collation).
pub fn word_id_by_lemma(conn: &Connection, lemma: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT word_id FROM words WHERE lemma = ?1",
            rusqlite::params![lemma],
            |row| row.get(0),
        )
        .optional()?)
}

/// Current plan summary, if a plan has been built.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSummary {
    pub plan_id: i64,
    pub built_at: String,
    pub group_count: i64,
    pub word_count: i64,
}

pub fn current_plan(conn: &Connection) -> Result<Option<PlanSummary>> {
    let plan = conn
        .query_row(
            "SELECT plan_id, built_at FROM plan_artifacts WHERE is_current = 1",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((plan_id, built_at)) = plan else {
        return Ok(None);
    };
    let group_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM plan_groups WHERE plan_id = ?1",
        rusqlite::params![plan_id],
        |row| row.get(0),
    )?;
    let word_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM plan_words WHERE plan_id = ?1",
        rusqlite::params![plan_id],
        |row| row.get(0),
    )?;
    Ok(Some(PlanSummary {
        plan_id,
        built_at,
        group_count,
        word_count,
    }))
}
