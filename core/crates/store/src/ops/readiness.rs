//! Readiness cache writes: `words.ready`, `words.core_ready`, `words.blockers`.
//!
//! Readiness is pure database mathematics recomputed inline every pass
//! (README Part 3 §"任务、事件、就绪度"), so these columns are a cache, not a
//! source of truth. The write is therefore a plain diff-and-update: only rows
//! whose computed value actually changed are touched, which keeps the change
//! bus quiet once the system has converged.

use std::collections::HashMap;

use morpho_domain::change::EntityType;

use super::{OpCtx, WriteResult};
use crate::error::Result;

/// One word's recomputed readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessRow {
    pub word_id: i64,
    pub ready: bool,
    pub core_ready: bool,
    /// JSON array, already in canonical order (`BlockerSet::to_json`).
    pub blockers_json: String,
}

/// Apply a full readiness recomputation.
#[derive(Debug, Clone)]
pub struct ApplyReadiness {
    pub rows: Vec<ReadinessRow>,
}

pub(super) fn apply_readiness(req: ApplyReadiness, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let mut current: HashMap<i64, (i64, i64, String)> = HashMap::new();
    {
        let mut stmt = ctx
            .tx
            .prepare("SELECT word_id, ready, core_ready, blockers FROM words")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (word_id, ready, core_ready, blockers) = row?;
            current.insert(word_id, (ready, core_ready, blockers));
        }
    }

    let mut changed = 0usize;
    {
        let mut stmt = ctx.tx.prepare(
            "UPDATE words SET ready = ?2, core_ready = ?3, blockers = ?4 WHERE word_id = ?1",
        )?;
        for row in &req.rows {
            let desired = (
                i64::from(row.ready),
                i64::from(row.core_ready),
                row.blockers_json.clone(),
            );
            if current.get(&row.word_id) == Some(&desired) {
                continue;
            }
            stmt.execute(rusqlite::params![
                row.word_id,
                desired.0,
                desired.1,
                desired.2
            ])?;
            changed += 1;
            ctx.touch(EntityType::Word, row.word_id.to_string());
        }
    }

    Ok(WriteResult::Readiness { changed })
}
