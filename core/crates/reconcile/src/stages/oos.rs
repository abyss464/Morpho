//! Synchronizing the human-facing OOV queue with the `oos_occurrences` view.
//!
//! The view is the truth and can never be stale; the queue is the workflow
//! state a person acts on. Reconciling them is a set difference in both
//! directions (README Part 3 §"派生 · 分词与依赖").

use morpho_domain::event::Actor;
use morpho_store::error::Result;
use morpho_store::ops::SyncOosQueue;
use morpho_store::{queries, Store, WriteOp, WriteResult};

/// Returns `(opened, auto_closed)`.
pub async fn sync(store: &Store) -> Result<(usize, usize)> {
    let present = store.read(queries::oos_lemmas).await?;
    let outcome = store
        .write(
            Actor::Reconciler,
            WriteOp::SyncOosQueue(SyncOosQueue { present }),
        )
        .await?;
    Ok(match outcome.result {
        WriteResult::OosSync { opened, closed } => (opened, closed),
        _ => (0, 0),
    })
}
