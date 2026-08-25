//! The single writer task.
//!
//! Exactly one connection in the whole process may write, and it lives on a
//! dedicated OS thread owned by this module. Every request is one transaction;
//! the change bus is published only after the commit succeeds. Because our own
//! writes are serialized by construction and no other process opens the file,
//! `SQLITE_BUSY` between our writers is structurally impossible
//! (README Part 4).

use std::path::PathBuf;

use rusqlite::Connection;
use tokio::sync::{broadcast, mpsc, oneshot};

use morpho_domain::change::{ChangeEvent, ChangeSet};
use morpho_domain::event::Actor;
use morpho_domain::time::now_ts;

use crate::error::{Result, StoreError};
use crate::ops::{apply_op, OpCtx, WriteOp, WriteResult};

/// What a completed [`WriteOp`] reports back.
#[derive(Debug, Clone)]
pub struct WriteOutcome {
    pub result: WriteResult,
    /// Entity keys touched by the committed transaction, as published on the bus.
    pub changes: Vec<ChangeEvent>,
    /// Number of audit-log rows written inside the same transaction.
    pub events_written: usize,
}

pub(crate) struct WriteRequest {
    pub actor: Actor,
    pub op: WriteOp,
    pub ack: oneshot::Sender<Result<WriteOutcome>>,
}

/// Spawn the writer thread. Returns the sender half of the request channel.
pub(crate) fn spawn(
    conn: Connection,
    path: PathBuf,
    queue_depth: usize,
    bus: broadcast::Sender<ChangeEvent>,
) -> mpsc::Sender<WriteRequest> {
    let (tx, rx) = mpsc::channel::<WriteRequest>(queue_depth);
    std::thread::Builder::new()
        .name("morphod-writer".to_string())
        .spawn(move || writer_loop(conn, path, rx, bus))
        .expect("failed to spawn the store writer thread");
    tx
}

fn writer_loop(
    mut conn: Connection,
    path: PathBuf,
    mut rx: mpsc::Receiver<WriteRequest>,
    bus: broadcast::Sender<ChangeEvent>,
) {
    tracing::debug!(path = %path.display(), "store writer started");
    while let Some(request) = rx.blocking_recv() {
        let WriteRequest { actor, op, ack } = request;
        let outcome = apply_transaction(&mut conn, &actor, op);
        if let Ok(outcome) = &outcome {
            // Publish only after the commit. A missing subscriber is fine: the
            // periodic full pass is what guarantees convergence.
            for change in &outcome.changes {
                let _ = bus.send(change.clone());
            }
        }
        let _ = ack.send(outcome);
    }
    tracing::debug!("store writer stopped");
}

fn apply_transaction(conn: &mut Connection, actor: &Actor, op: WriteOp) -> Result<WriteOutcome> {
    let tx = conn.transaction()?;
    let mut changes = ChangeSet::new();
    let (result, events_written) = {
        let mut ctx = OpCtx {
            tx: &tx,
            actor,
            changes: &mut changes,
            now: now_ts(),
            events_written: 0,
        };
        let result = apply_op(op, &mut ctx)?;
        (result, ctx.events_written)
    };
    tx.commit()?;
    Ok(WriteOutcome {
        result,
        changes: changes.into_events(),
        events_written,
    })
}

/// Send one operation to the writer and await its acknowledgement.
pub(crate) async fn submit(
    tx: &mpsc::Sender<WriteRequest>,
    actor: Actor,
    op: WriteOp,
) -> Result<WriteOutcome> {
    let (ack, rx) = oneshot::channel();
    tx.send(WriteRequest { actor, op, ack })
        .await
        .map_err(|_| StoreError::Closed)?;
    rx.await.map_err(|_| StoreError::Closed)?
}
