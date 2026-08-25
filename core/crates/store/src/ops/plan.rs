//! Learning-plan artifact writes.
//!
//! A plan is a versioned singleton (README Part 3 §"派生 · 学习计划"): the new
//! artifact, its groups and its word placements all land in one transaction,
//! and `is_current` moves atomically. Old artifacts are kept so the console can
//! diff them.
//!
//! Optimistic concurrency: the builder computed the plan from a read snapshot
//! and carries the `input_hash` it saw. If a plan with that hash is already
//! current, the write is a no-op — the state has not drifted, and rewriting
//! would only churn `plan_id`s.

use rusqlite::OptionalExtension;

use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, EventDraft};

use super::{OpCtx, WriteResult};
use crate::error::Result;

/// One group of the plan, in learning order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanGroupRow {
    pub group_seq: i64,
    pub group_type: String,
}

/// One word's position in the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanWordRow {
    pub word_id: i64,
    pub learning_order: i64,
    pub group_seq: i64,
}

/// A freshly computed plan, ready to become current.
#[derive(Debug, Clone)]
pub struct WritePlan {
    pub input_hash: String,
    pub algo_ver: String,
    pub params_json: String,
    pub stats_json: String,
    pub groups: Vec<PlanGroupRow>,
    pub words: Vec<PlanWordRow>,
}

pub(super) fn write_plan(req: WritePlan, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let current: Option<(i64, String)> = ctx
        .tx
        .query_row(
            "SELECT plan_id, input_hash FROM plan_artifacts WHERE is_current = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    if let Some((plan_id, hash)) = &current {
        if hash == &req.input_hash {
            return Ok(WriteResult::Plan {
                plan_id: *plan_id,
                applied: false,
            });
        }
    }

    ctx.tx.execute(
        "UPDATE plan_artifacts SET is_current = 0 WHERE is_current = 1",
        [],
    )?;
    ctx.tx.execute(
        "INSERT INTO plan_artifacts (input_hash, algo_ver, params_json, is_current, built_at, stats_json)
         VALUES (?1, ?2, ?3, 1, ?4, ?5)",
        rusqlite::params![
            req.input_hash,
            req.algo_ver,
            req.params_json,
            ctx.now,
            req.stats_json
        ],
    )?;
    let plan_id = ctx.tx.last_insert_rowid();

    {
        let mut stmt = ctx.tx.prepare(
            "INSERT INTO plan_groups (plan_id, group_seq, group_type) VALUES (?1, ?2, ?3)",
        )?;
        for group in &req.groups {
            stmt.execute(rusqlite::params![
                plan_id,
                group.group_seq,
                group.group_type
            ])?;
        }
    }
    {
        let mut stmt = ctx.tx.prepare(
            "INSERT INTO plan_words (plan_id, word_id, learning_order, group_seq)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for word in &req.words {
            stmt.execute(rusqlite::params![
                plan_id,
                word.word_id,
                word.learning_order,
                word.group_seq
            ])?;
        }
    }

    ctx.event(
        EventDraft::new(EntityType::Plan, plan_id.to_string(), Action::PlanRebuilt).detail(
            serde_json::json!({
                "plan_id": plan_id,
                "previous_plan_id": current.as_ref().map(|(id, _)| *id),
                "input_hash": req.input_hash,
                "algo_ver": req.algo_ver,
                "word_count": req.words.len(),
                "group_count": req.groups.len(),
            }),
        ),
    )?;
    ctx.touch(EntityType::Plan, plan_id.to_string());

    Ok(WriteResult::Plan {
        plan_id,
        applied: true,
    })
}
