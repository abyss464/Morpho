//! Auxiliary word liveness (README Part 3 §"辅助词生命周期").
//!
//! An auxiliary exists only because something live points at it: a dependency
//! edge from an enabled definition, or a distractor binding, of a word that a
//! target reaches. A reference from a retired word, or a cycle of auxiliaries
//! nothing live points into, does not count. When the last live reference goes
//! the word is retired — it leaves `active_words`, the plan, the TTS desired
//! set and every release. When one comes back it is reactivated, assets
//! intact.
//!
//! Both directions are reversible and destroy nothing. The `aux_liveness` view
//! is a pure derivation, so this stage is a diff between the view and the
//! stored status, never a decision.

use morpho_domain::event::Actor;
use morpho_domain::types::AuxStatus;
use morpho_store::error::Result;
use morpho_store::{Store, WriteOp};

/// Returns `(retired, reactivated)`.
pub async fn sync(store: &Store) -> Result<(usize, usize)> {
    let transitions = store
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT w.word_id, w.aux_status, l.is_live
                 FROM words w
                 JOIN aux_liveness l ON l.word_id = w.word_id
                 WHERE w.role = 'auxiliary'
                 ORDER BY w.word_id",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?;

    let mut ops = Vec::new();
    let mut retired = 0usize;
    let mut reactivated = 0usize;
    for (word_id, status, is_live) in transitions {
        let active = status.as_deref() == Some(AuxStatus::Active.as_str());
        match (active, is_live) {
            (true, false) => {
                ops.push(WriteOp::retire_aux(word_id, "no live reference"));
                retired += 1;
            }
            (false, true) => {
                ops.push(WriteOp::reactivate_aux(word_id, "referenced again"));
                reactivated += 1;
            }
            _ => {}
        }
    }

    if ops.is_empty() {
        return Ok((0, 0));
    }
    store.write(Actor::Reconciler, WriteOp::Batch(ops)).await?;
    Ok((retired, reactivated))
}
