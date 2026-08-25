//! Distractor binding.
//!
//! Product rule (README Part 3 §"派生 · 干扰项"): distractors are bound once and
//! never change. This table is deliberately exempt from the staleness
//! machinery, so the engine only ever *inserts* rows for words that lack them —
//! there is no update path here at all. Replacing a binding is an explicit
//! human action through a different route.

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
