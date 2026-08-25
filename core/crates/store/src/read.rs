//! Read-only connection pool.
//!
//! rusqlite is blocking, so every read runs on `spawn_blocking` with a
//! connection checked out of the pool. A semaphore bounds concurrency to the
//! pool size, which means `pop()` never fails while the pool is healthy.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use tokio::sync::Semaphore;

use crate::conn::open_read_connection;
use crate::error::{Result, StoreError};

/// Number of read connections when the caller does not choose (README Part 4
/// says 4–8).
pub const DEFAULT_READ_POOL_SIZE: usize = 6;

pub struct ReadPool {
    path: PathBuf,
    conns: Mutex<Vec<Connection>>,
    permits: Semaphore,
}

impl ReadPool {
    pub fn open(path: &Path, size: usize) -> Result<Self> {
        let size = size.max(1);
        let mut conns = Vec::with_capacity(size);
        for _ in 0..size {
            conns.push(open_read_connection(path)?);
        }
        Ok(Self {
            path: path.to_path_buf(),
            conns: Mutex::new(conns),
            permits: Semaphore::new(size),
        })
    }

    pub fn size(&self) -> usize {
        self.permits.available_permits()
    }

    /// Run `f` on a pooled read-only connection.
    pub async fn with<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| StoreError::Closed)?;

        let conn = {
            let mut guard = self.conns.lock().expect("read pool mutex poisoned");
            match guard.pop() {
                Some(conn) => conn,
                None => {
                    // Only reachable if a previous blocking task panicked and
                    // leaked its connection; rebuild one rather than deadlock.
                    open_read_connection(&self.path)?
                }
            }
        };

        let joined = tokio::task::spawn_blocking(move || {
            let result = f(&conn);
            (conn, result)
        })
        .await;

        match joined {
            Ok((conn, result)) => {
                self.conns
                    .lock()
                    .expect("read pool mutex poisoned")
                    .push(conn);
                drop(permit);
                result
            }
            Err(err) => {
                // The connection went down with the panicking task. Drop the
                // permit anyway; `with` re-opens lazily on the next miss.
                drop(permit);
                Err(StoreError::Background(err.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conn::open_write_connection;
    use crate::schema::ensure_schema;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("working.db");
        let mut conn = open_write_connection(&path).unwrap();
        ensure_schema(&mut conn).unwrap();
        (dir, path)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_reads_share_the_pool() {
        let (_dir, path) = fixture();
        let pool = std::sync::Arc::new(ReadPool::open(&path, 3).unwrap());
        let mut handles = Vec::new();
        for _ in 0..16 {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move {
                pool.with(|conn| {
                    let n: i64 = conn.query_row("SELECT COUNT(*) FROM words", [], |r| r.get(0))?;
                    Ok(n)
                })
                .await
                .unwrap()
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), 0);
        }
        assert_eq!(pool.size(), 3);
    }

    #[tokio::test]
    async fn survives_a_panicking_read() {
        let (_dir, path) = fixture();
        let pool = ReadPool::open(&path, 1).unwrap();
        let boom = pool
            .with(|_conn| -> Result<()> { panic!("boom") })
            .await
            .unwrap_err();
        assert!(matches!(boom, StoreError::Background(_)));
        let n = pool
            .with(|conn| {
                let n: i64 = conn.query_row("SELECT COUNT(*) FROM words", [], |r| r.get(0))?;
                Ok(n)
            })
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
}
