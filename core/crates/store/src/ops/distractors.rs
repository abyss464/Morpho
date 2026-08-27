//! Distractor binding.
//!
//! Product rule (README Part 3 §"派生 · 干扰项"): distractors are bound once and
//! never change. This table is deliberately exempt from the staleness
//! machinery, so [`bind_distractors`] only ever *inserts* rows for words that
//! lack them — no automatic path here rewrites one.
//!
//! [`rebind_distractors`] is the other half of that rule: replacing a binding
//! is an explicit human action, which is why it is a separate operation, takes
//! a reason, records the actor in `bound_by`, and writes one event per row it
//! moves. It is never reachable from the reconciler.

use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, EventDraft};

use super::{OpCtx, WriteResult};
use crate::error::{Result, StoreError};

/// One word's three distractors, in rank order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistractorBinding {
    pub word_id: i64,
    /// `(rank, distractor_word_id)` pairs; rank ∈ 1..=3.
    pub ranks: Vec<(i64, i64)>,
}

/// Insert missing distractor bindings.
#[derive(Debug, Clone)]
pub struct BindDistractors {
    pub bindings: Vec<DistractorBinding>,
    pub algo_ver: String,
}

/// One rank moving from one distractor to another.
///
/// `old_distractor_word_id` is an optimistic guard, not decoration: the row is
/// only rewritten if it still holds the word the plan was computed against, so
/// a plan that raced another edit is discarded rank by rank instead of
/// overwriting somebody's work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistractorRebind {
    pub word_id: i64,
    pub rank: i64,
    pub old_distractor_word_id: i64,
    pub new_distractor_word_id: i64,
}

/// Replace existing distractor bindings — the sanctioned human repair path.
#[derive(Debug, Clone)]
pub struct RebindDistractors {
    pub rebinds: Vec<DistractorRebind>,
    pub algo_ver: String,
    /// Why the bindings moved, recorded verbatim in every event.
    pub reason: String,
}

pub(super) fn bind_distractors(
    req: BindDistractors,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    let mut bound = 0usize;
    for binding in &req.bindings {
        let existing: i64 = ctx.tx.query_row(
            "SELECT COUNT(*) FROM distractors WHERE word_id = ?1",
            rusqlite::params![binding.word_id],
            |row| row.get(0),
        )?;
        if existing > 0 {
            // Never touch an existing binding, not even a partial one: a
            // half-bound word keeps its ranks and only gains the missing ones.
            let taken: Vec<i64> = {
                let mut stmt = ctx
                    .tx
                    .prepare("SELECT rank FROM distractors WHERE word_id = ?1")?;
                let rows = stmt
                    .query_map(rusqlite::params![binding.word_id], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<i64>>>()?;
                rows
            };
            if binding.ranks.iter().all(|(rank, _)| taken.contains(rank)) {
                continue;
            }
        }

        let mut inserted_ranks = Vec::new();
        for (rank, distractor_word_id) in &binding.ranks {
            if !(1..=3).contains(rank) {
                return Err(StoreError::invalid(format!(
                    "distractor rank {rank} is outside 1..=3"
                )));
            }
            if *distractor_word_id == binding.word_id {
                return Err(StoreError::invalid(format!(
                    "word {} cannot distract itself",
                    binding.word_id
                )));
            }
            let inserted = ctx.tx.execute(
                "INSERT INTO distractors (word_id, rank, distractor_word_id, algo_ver, bound_at, bound_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'auto')
                 ON CONFLICT (word_id, rank) DO NOTHING",
                rusqlite::params![
                    binding.word_id,
                    rank,
                    distractor_word_id,
                    req.algo_ver,
                    ctx.now
                ],
            )?;
            if inserted > 0 {
                inserted_ranks.push((*rank, *distractor_word_id));
            }
        }

        if !inserted_ranks.is_empty() {
            bound += inserted_ranks.len();
            ctx.event(
                EventDraft::new(
                    EntityType::Distractor,
                    binding.word_id.to_string(),
                    Action::DistractorBound,
                )
                .detail(serde_json::json!({
                    "word_id": binding.word_id,
                    "algo_ver": req.algo_ver,
                    "bound": inserted_ranks
                        .iter()
                        .map(|(rank, id)| serde_json::json!({"rank": rank, "distractor_word_id": id}))
                        .collect::<Vec<_>>(),
                })),
            )?;
            ctx.touch(EntityType::Distractor, binding.word_id.to_string());
            ctx.touch(EntityType::Word, binding.word_id.to_string());
        }
    }
    Ok(WriteResult::Bound { bound })
}

pub(super) fn rebind_distractors(
    req: RebindDistractors,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    let mut rebound = 0usize;
    let mut skipped = 0usize;
    let bound_by = ctx.actor.to_string();

    for rebind in &req.rebinds {
        if !(1..=3).contains(&rebind.rank) {
            return Err(StoreError::invalid(format!(
                "distractor rank {} is outside 1..=3",
                rebind.rank
            )));
        }
        if rebind.new_distractor_word_id == rebind.word_id {
            return Err(StoreError::invalid(format!(
                "word {} cannot distract itself",
                rebind.word_id
            )));
        }

        // `UNIQUE (word_id, distractor_word_id)`: a replacement the word already
        // holds at another rank would abort the whole transaction, so it is
        // dropped here instead. The no-op case (the row already points at the
        // replacement) lands in the same branch, which makes a retried plan
        // idempotent.
        let clash: i64 = ctx.tx.query_row(
            "SELECT COUNT(*) FROM distractors WHERE word_id = ?1 AND distractor_word_id = ?2",
            rusqlite::params![rebind.word_id, rebind.new_distractor_word_id],
            |row| row.get(0),
        )?;
        if clash > 0 {
            skipped += 1;
            continue;
        }

        let changed = ctx.tx.execute(
            "UPDATE distractors
                SET distractor_word_id = ?1, algo_ver = ?2, bound_at = ?3, bound_by = ?4
              WHERE word_id = ?5 AND rank = ?6 AND distractor_word_id = ?7",
            rusqlite::params![
                rebind.new_distractor_word_id,
                req.algo_ver,
                ctx.now,
                bound_by,
                rebind.word_id,
                rebind.rank,
                rebind.old_distractor_word_id
            ],
        )?;
        if changed == 0 {
            // The row moved (or vanished) between planning and writing.
            skipped += 1;
            continue;
        }

        rebound += 1;
        ctx.event(
            EventDraft::new(
                EntityType::Distractor,
                rebind.word_id.to_string(),
                Action::DistractorBound,
            )
            .detail(serde_json::json!({
                "word_id": rebind.word_id,
                "rank": rebind.rank,
                "old_distractor_word_id": rebind.old_distractor_word_id,
                "new_distractor_word_id": rebind.new_distractor_word_id,
                "algo_ver": req.algo_ver,
                "reason": req.reason,
            })),
        )?;
        ctx.touch(EntityType::Distractor, rebind.word_id.to_string());
        ctx.touch(EntityType::Word, rebind.word_id.to_string());
    }

    Ok(WriteResult::Rebound { rebound, skipped })
}
