//! Connection construction and the runtime PRAGMAs mandated by the
//! working-database contract header:
//! `journal_mode=WAL, synchronous=NORMAL, foreign_keys=ON, busy_timeout=5000`.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::error::Result;

/// `busy_timeout` in milliseconds, per contract.
pub const BUSY_TIMEOUT_MS: i64 = 5_000;

/// Open the single read/write connection. Only the writer task may hold one.
pub fn open_write_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    apply_common_pragmas(&conn)?;
    // journal_mode returns a row, so it cannot go through pragma_update.
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        tracing::warn!(journal_mode = %mode, "database did not accept WAL journal mode");
    }
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(conn)
}

/// Open one pooled read-only connection.
pub fn open_read_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    apply_common_pragmas(&conn)?;
    // Belt and braces: refuse writes even if a query tries.
    conn.pragma_update(None, "query_only", true)?;
    Ok(conn)
}

fn apply_common_pragmas(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "busy_timeout", BUSY_TIMEOUT_MS)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ensure_schema;

    #[test]
    fn write_connection_gets_contract_pragmas() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("working.db");
        let mut conn = open_write_connection(&path).unwrap();
        ensure_schema(&mut conn).unwrap();

        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(journal.to_lowercase(), "wal");
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let busy: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy, BUSY_TIMEOUT_MS);
        let sync: i64 = conn
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sync, 1, "synchronous=NORMAL");
    }

    #[test]
    fn read_connection_refuses_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("working.db");
        {
            let mut conn = open_write_connection(&path).unwrap();
            ensure_schema(&mut conn).unwrap();
        }
        let reader = open_read_connection(&path).unwrap();
        let err = reader
            .execute(
                "INSERT INTO words (lemma, role) VALUES ('nope', 'target')",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("read"));
    }
}
