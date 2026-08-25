//! The `Store` handle: the only way any other subsystem reaches SQLite.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::{broadcast, mpsc};

use morpho_domain::change::ChangeEvent;
use morpho_domain::event::Actor;

use crate::conn::open_write_connection;
use crate::error::Result;
use crate::ops::WriteOp;
use crate::read::{ReadPool, DEFAULT_READ_POOL_SIZE};
use crate::schema::ensure_schema;
use crate::writer::{self, WriteOutcome, WriteRequest};

/// Store construction parameters.
#[derive(Debug, Clone)]
pub struct StoreConfig {
    /// Path of `working.db`. Its parent directory is created if missing.
    pub path: PathBuf,
    /// Read-only connections in the pool (README Part 4: 4–8).
    pub read_pool_size: usize,
    /// Bound on queued write operations before callers start waiting.
    pub write_queue_depth: usize,
    /// Change-bus capacity; slow subscribers get lagged, never block the writer.
    pub change_bus_capacity: usize,
}

impl StoreConfig {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            read_pool_size: DEFAULT_READ_POOL_SIZE,
            write_queue_depth: 256,
            change_bus_capacity: 1024,
        }
    }
}

/// Cloneable handle to the working database.
#[derive(Clone)]
pub struct Store {
    inner: Arc<Inner>,
}

struct Inner {
    path: PathBuf,
    writes: mpsc::Sender<WriteRequest>,
    reads: ReadPool,
    bus: broadcast::Sender<ChangeEvent>,
    created: bool,
}

impl Store {
    /// Open (creating if needed) the working database and start the writer task.
    pub fn open(config: StoreConfig) -> Result<Self> {
        if let Some(parent) = config.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut conn = open_write_connection(&config.path)?;
        let created = ensure_schema(&mut conn)?;
        if created {
            tracing::info!(path = %config.path.display(), "created working database");
        }

        let reads = ReadPool::open(&config.path, config.read_pool_size)?;
        let (bus, _) = broadcast::channel(config.change_bus_capacity);
        let writes = writer::spawn(
            conn,
            config.path.clone(),
            config.write_queue_depth,
            bus.clone(),
        );

        Ok(Self {
            inner: Arc::new(Inner {
                path: config.path,
                writes,
                reads,
                bus,
                created,
            }),
        })
    }

    /// Path of the database file.
    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    /// True when this process created the database file.
    pub fn was_created(&self) -> bool {
        self.inner.created
    }

    /// Submit one write operation. Resolves after the transaction commits.
    pub async fn write(&self, actor: Actor, op: WriteOp) -> Result<WriteOutcome> {
        writer::submit(&self.inner.writes, actor, op).await
    }

    /// Run a read-only query on a pooled connection.
    pub async fn read<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.inner.reads.with(f).await
    }

    /// Subscribe to the change bus.
    pub fn subscribe(&self) -> broadcast::Receiver<ChangeEvent> {
        self.inner.bus.subscribe()
    }

    /// Number of live change-bus subscribers (diagnostics).
    pub fn subscriber_count(&self) -> usize {
        self.inner.bus.receiver_count()
    }
}
