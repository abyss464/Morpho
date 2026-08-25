//! Schema bootstrap and migration.
//!
//! `docs/contracts/working-db.sql` is the normative DDL and is embedded
//! verbatim — morphod never carries a second copy of the schema. Runtime
//! PRAGMAs come from that file's header comment and live in [`crate::conn`].
//!
//! A fresh database is created straight from the contract and stamped with the
//! current [`SCHEMA_USER_VERSION`]. An older database is walked forward one
//! step at a time by [`MIGRATIONS`]; every step must leave the database
//! semantically identical to one created by the contract at that version —
//! same objects, same columns, same constraints. Physical column *order* may
//! differ, because `ALTER TABLE ADD COLUMN` appends: SQLite re-expands `SELECT *`
//! inside a view at prepare time, so `active_words` picks the new column up
//! either way, and no query in morphod depends on ordinal positions.

use rusqlite::Connection;

use crate::error::{Result, StoreError};
use morpho_domain::version::SCHEMA_USER_VERSION;

/// The normative working-database DDL, embedded at compile time.
pub const WORKING_DB_SQL: &str = include_str!("../../../../docs/contracts/working-db.sql");

/// Dispatcher lane limits. These mirror the normative seed rows at the bottom
/// of `docs/contracts/working-db.sql` exactly; they are applied with
/// `INSERT OR IGNORE` semantics so operator tuning survives every restart.
pub const CONTRACT_RATE_LIMITS: &[(&str, i64, f64, i64)] = &[
    // (rate_key, max_concurrency, refill_per_min, burst)
    ("freedict", 2, 120.0, 4),
    ("wiktionary", 1, 60.0, 2),
    ("unsplash", 2, 45.0, 4),
    ("pexels", 2, 180.0, 4),
    ("pixabay", 2, 90.0, 4),
    ("sdxl", 1, 6.0, 1),
    ("edge_tts", 4, 240.0, 8),
    ("llm", 2, 30.0, 4),
    ("cpu", 8, 6000.0, 16),
];

/// One forward migration step: `(from_version, statements)`.
///
/// Each entry upgrades a database at `from_version` to `from_version + 1`.
pub const MIGRATIONS: &[(i32, &[&str])] = &[(
    1,
    // Wave 2: the reconciler owns a materialized `core_ready` so
    // `DistractorView.core_ready` is a column read rather than a derivation
    // hack (admin-api.md wave-2 ruling #5).
    &["ALTER TABLE words ADD COLUMN core_ready INTEGER NOT NULL DEFAULT 0"],
)];

/// Create the schema if the database is empty, migrate it if it is older, then
/// stamp `user_version`.
///
/// Returns `true` when the schema was created by this call.
pub fn ensure_schema(conn: &mut Connection) -> Result<bool> {
    let has_tables: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name='words')",
        [],
        |row| row.get::<_, i64>(0).map(|v| v != 0),
    )?;

    if !has_tables {
        let tx = conn.transaction()?;
        tx.execute_batch(WORKING_DB_SQL)?;
        tx.commit()?;
        conn.pragma_update(None, "user_version", SCHEMA_USER_VERSION)?;
        seed_rate_limits(conn)?;
        return Ok(true);
    }

    migrate(conn)?;
    seed_rate_limits(conn)?;
    Ok(false)
}

/// Walk an existing database forward to [`SCHEMA_USER_VERSION`].
pub fn migrate(conn: &mut Connection) -> Result<()> {
    let mut version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    // A database that predates versioning reports 0. Wave-1 databases were
    // stamped 1; treat 0 as 1 and let the ladder do the rest.
    if version == 0 {
        version = 1;
    }

    if version > SCHEMA_USER_VERSION {
        return Err(StoreError::conflict(format!(
            "working database is at schema version {version}, but this build only \
             understands {SCHEMA_USER_VERSION}"
        )));
    }

    while version < SCHEMA_USER_VERSION {
        let step = MIGRATIONS
            .iter()
            .find(|(from, _)| *from == version)
            .ok_or_else(|| {
                StoreError::conflict(format!("no migration from schema version {version}"))
            })?;
        let tx = conn.transaction()?;
        for statement in step.1 {
            tx.execute_batch(statement)?;
        }
        tx.commit()?;
        version += 1;
        conn.pragma_update(None, "user_version", version)?;
        tracing::info!(version, "migrated working database");
    }

    conn.pragma_update(None, "user_version", SCHEMA_USER_VERSION)?;
    Ok(())
}

/// Insert any missing lane defaults. Existing rows are left untouched so an
/// operator's tuning survives restarts.
pub fn seed_rate_limits(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO rate_limits (rate_key, max_concurrency, refill_per_min, burst)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (rate_key) DO NOTHING",
    )?;
    for (key, concurrency, refill, burst) in CONTRACT_RATE_LIMITS {
        stmt.execute(rusqlite::params![key, concurrency, refill, burst])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wave-1 database: the contract DDL minus everything wave 2 added.
    fn legacy_v1(conn: &mut Connection) {
        let mut ddl = WORKING_DB_SQL.to_string();

        let core_ready_line = ddl
            .lines()
            .find(|line| line.trim_start().starts_with("core_ready"))
            .expect("contract declares core_ready")
            .to_string();
        ddl = ddl.replace(&format!("{core_ready_line}\n"), "");
        assert!(!ddl.contains("core_ready"));

        // Wave-1 databases were created before the seed rows existed.
        let start = ddl
            .find("INSERT OR IGNORE INTO rate_limits")
            .expect("contract seeds rate_limits");
        let end = ddl[start..].find(';').expect("statement is terminated") + start + 1;
        ddl.replace_range(start..end, "");

        conn.execute_batch(&ddl).expect("legacy ddl");
        conn.pragma_update(None, "user_version", 1).unwrap();
    }

    fn columns(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }

    #[test]
    fn embedded_ddl_is_the_contract_file() {
        assert!(WORKING_DB_SQL.contains("CREATE TABLE words ("));
        assert!(WORKING_DB_SQL.contains("CREATE VIEW active_words"));
        assert!(WORKING_DB_SQL.contains("CREATE TABLE release_manifests"));
        assert!(WORKING_DB_SQL.contains("core_ready"));
    }

    #[test]
    fn creates_then_recognizes_the_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert!(ensure_schema(&mut conn).unwrap());
        assert!(!ensure_schema(&mut conn).unwrap());
        let ver: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver, SCHEMA_USER_VERSION);
    }

    #[test]
    fn every_contract_object_exists_after_bootstrap() {
        let mut conn = Connection::open_in_memory().unwrap();
        ensure_schema(&mut conn).unwrap();
        for (kind, name) in [
            ("table", "words"),
            ("table", "definition_candidates"),
            ("table", "definition_selections"),
            ("table", "example_candidates"),
            ("table", "example_selections"),
            ("table", "image_candidates"),
            ("table", "image_selections"),
            ("table", "media_files"),
            ("table", "def_extractions"),
            ("table", "def_tokens"),
            ("table", "oos_queue"),
            ("table", "tts_assets"),
            ("table", "plan_artifacts"),
            ("table", "plan_groups"),
            ("table", "plan_words"),
            ("table", "distractors"),
            ("table", "job_state"),
            ("table", "source_fetch"),
            ("table", "rate_limits"),
            ("table", "events"),
            ("table", "releases"),
            ("table", "release_manifests"),
            ("view", "active_words"),
            ("view", "def_dependencies"),
            ("view", "oos_occurrences"),
            ("view", "tts_desired"),
            ("view", "aux_liveness"),
        ] {
            let found: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                    rusqlite::params![kind, name],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(found, 1, "missing {kind} {name}");
        }
    }

    #[test]
    fn fresh_database_carries_the_contract_seed_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        ensure_schema(&mut conn).unwrap();
        for (key, concurrency, refill, burst) in CONTRACT_RATE_LIMITS {
            let row: (i64, f64, i64) = conn
                .query_row(
                    "SELECT max_concurrency, refill_per_min, burst FROM rate_limits
                     WHERE rate_key = ?1",
                    rusqlite::params![key],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap_or_else(|_| panic!("missing seed row {key}"));
            assert_eq!(row, (*concurrency, *refill, *burst), "seed drift for {key}");
        }
    }

    #[test]
    fn seeds_lane_defaults_idempotently() {
        let mut conn = Connection::open_in_memory().unwrap();
        ensure_schema(&mut conn).unwrap();
        conn.execute(
            "UPDATE rate_limits SET max_concurrency = 99 WHERE rate_key = 'cpu'",
            [],
        )
        .unwrap();
        seed_rate_limits(&conn).unwrap();
        let concurrency: i64 = conn
            .query_row(
                "SELECT max_concurrency FROM rate_limits WHERE rate_key = 'cpu'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(concurrency, 99, "operator tuning must survive reseeding");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rate_limits", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count as usize, CONTRACT_RATE_LIMITS.len());
    }

    #[test]
    fn migrates_a_wave_one_database_in_place() {
        let mut conn = Connection::open_in_memory().unwrap();
        legacy_v1(&mut conn);
        assert!(!columns(&conn, "words").contains(&"core_ready".to_string()));
        conn.execute(
            "INSERT INTO words (lemma, role, ready, blockers) VALUES ('benevolent','target',1,'[]')",
            [],
        )
        .unwrap();
        let empty: i64 = conn
            .query_row("SELECT COUNT(*) FROM rate_limits", [], |r| r.get(0))
            .unwrap();
        assert_eq!(empty, 0, "fixture should start without seed rows");

        assert!(!ensure_schema(&mut conn).unwrap());

        assert!(columns(&conn, "words").contains(&"core_ready".to_string()));
        let ver: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver, SCHEMA_USER_VERSION);
        // Existing data survives, with the new column defaulted.
        let (lemma, ready, core_ready): (String, i64, i64) = conn
            .query_row("SELECT lemma, ready, core_ready FROM words", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!((lemma.as_str(), ready, core_ready), ("benevolent", 1, 0));
        // And the migration backfills the seed rows the old build never had.
        let seeded: i64 = conn
            .query_row("SELECT COUNT(*) FROM rate_limits", [], |r| r.get(0))
            .unwrap();
        assert_eq!(seeded as usize, CONTRACT_RATE_LIMITS.len());
    }

    #[test]
    fn migrated_schema_matches_a_freshly_created_one() {
        let mut migrated = Connection::open_in_memory().unwrap();
        legacy_v1(&mut migrated);
        ensure_schema(&mut migrated).unwrap();

        let mut fresh = Connection::open_in_memory().unwrap();
        ensure_schema(&mut fresh).unwrap();

        let mut a = columns(&migrated, "words");
        let mut b = columns(&fresh, "words");
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }

    #[test]
    fn the_active_words_view_sees_the_migrated_column() {
        // `SELECT *` inside a view is expanded when a statement is prepared, so
        // a view created before the ALTER still exposes the new column.
        let mut conn = Connection::open_in_memory().unwrap();
        legacy_v1(&mut conn);
        conn.execute(
            "INSERT INTO words (lemma, role, ready, blockers) VALUES ('serene','target',0,'[]')",
            [],
        )
        .unwrap();
        ensure_schema(&mut conn).unwrap();
        conn.execute("UPDATE words SET core_ready = 1", []).unwrap();
        let core_ready: i64 = conn
            .query_row("SELECT core_ready FROM active_words", [], |r| r.get(0))
            .unwrap();
        assert_eq!(core_ready, 1);
    }

    #[test]
    fn migration_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        legacy_v1(&mut conn);
        ensure_schema(&mut conn).unwrap();
        ensure_schema(&mut conn).unwrap();
        migrate(&mut conn).unwrap();
        let count = columns(&conn, "words")
            .iter()
            .filter(|c| *c == "core_ready")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn refuses_a_database_from_the_future() {
        let mut conn = Connection::open_in_memory().unwrap();
        ensure_schema(&mut conn).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_USER_VERSION + 1)
            .unwrap();
        let err = ensure_schema(&mut conn).unwrap_err();
        assert!(matches!(err, StoreError::Conflict(_)), "{err}");
    }

    #[test]
    fn migration_ladder_has_no_gaps() {
        for version in 1..SCHEMA_USER_VERSION {
            assert!(
                MIGRATIONS.iter().any(|(from, _)| *from == version),
                "no migration from version {version}"
            );
        }
    }
}
