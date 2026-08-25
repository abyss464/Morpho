//! Schema bootstrap and migration.
//!
//! `docs/contracts/working-db.sql` is the normative DDL and is embedded
//! verbatim — morphod never carries a second copy of the schema. Runtime
//! PRAGMAs come from that file's header comment and live in [`crate::conn`].
//!
//! A fresh database is created straight from the contract, stamped with
//! [`CONTRACT_SCHEMA_VERSION`] — the version that file describes — and then
//! walked forward by exactly the same ladder an existing database uses. An
//! older database enters the ladder wherever it happens to be. Every step must
//! leave the database semantically identical to one created by the contract at
//! that version — same objects, same columns, same constraints. Physical column
//! *order* may differ, because `ALTER TABLE ADD COLUMN` appends: SQLite
//! re-expands `SELECT *` inside a view at prepare time, so `active_words` picks
//! the new column up either way, and no query in morphod depends on ordinal
//! positions.

use rusqlite::Connection;

use crate::error::{Result, StoreError};
use morpho_domain::version::{CONTRACT_SCHEMA_VERSION, SCHEMA_USER_VERSION};

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

/// A column one migration step introduces: `(table, column, definition)`.
pub type ColumnAdd = (&'static str, &'static str, &'static str);

/// One rung of the migration ladder: a database at `from` becomes `from + 1`.
///
/// Every rung so far is "add columns", and the type says so deliberately. It
/// buys two properties that a bag of SQL strings cannot:
///
/// * **idempotence** — [`Migration::apply`] checks before it alters, so a
///   contract file that has already caught up (the DDL ships the column, an old
///   database does not) can never brick the ladder with `duplicate column name`;
/// * **an exact inverse** — [`Migration::revert`] lets the tests fabricate a
///   genuine historical schema by walking the current contract *backwards*,
///   instead of doing string surgery on a file that legitimately evolves.
///
/// A future rung that needs more than columns adds a field here and its own
/// inverse alongside it. It does not get to smuggle DDL past the guard.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub from: i32,
    pub add_columns: &'static [ColumnAdd],
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        from: 1,
        // Wave 2: the reconciler owns a materialized `core_ready` so
        // `DistractorView.core_ready` is a column read rather than a derivation
        // hack (admin-api.md wave-2 ruling #5).
        add_columns: &[("words", "core_ready", "INTEGER NOT NULL DEFAULT 0")],
    },
    Migration {
        from: 2,
        // Wave 3: the release history owns its word count, so `GET /releases`
        // reads a column instead of re-parsing the audit log
        // (admin-api.md wave-3 ruling #15). Releases exported before this
        // migration keep the 0 default; their count only ever lived in the
        // event detail.
        add_columns: &[("releases", "word_count", "INTEGER NOT NULL DEFAULT 0")],
    },
];

/// The contract file may trail the code, never lead it: a DDL from the future
/// would be created and then have migrations replayed on top of it.
const _: () = assert!(CONTRACT_SCHEMA_VERSION <= SCHEMA_USER_VERSION);

impl Migration {
    /// Apply this rung. Columns that already exist are skipped, so the step is
    /// safe on a database created from a contract file that ships them.
    fn apply(&self, tx: &rusqlite::Transaction<'_>) -> Result<()> {
        for (table, column, definition) in self.add_columns {
            if has_column(tx, table, column)? {
                tracing::debug!(
                    table,
                    column,
                    "column already present; migration step skipped"
                );
                continue;
            }
            tx.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition}"
            ))?;
        }
        Ok(())
    }

    /// Undo this rung, so tests can fabricate the schema that preceded it.
    ///
    /// Lives next to [`Migration::apply`] on purpose: a new rung that forgets
    /// its inverse is a compile-time impossibility rather than a fixture that
    /// silently rots.
    #[cfg(test)]
    fn revert(&self, conn: &Connection) -> Result<()> {
        for (table, column, _) in self.add_columns {
            if !has_column(conn, table, column)? {
                continue;
            }
            conn.execute_batch(&format!("ALTER TABLE {table} DROP COLUMN {column}"))?;
        }
        Ok(())
    }
}

/// Does `table` already have `column`?
fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
        rusqlite::params![table, column],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

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
        // Stamp what the contract file actually describes, then let the ladder
        // add anything the code has since grown.
        conn.pragma_update(None, "user_version", CONTRACT_SCHEMA_VERSION)?;
        migrate(conn)?;
        seed_rate_limits(conn)?;
        return Ok(true);
    }

    migrate(conn)?;
    seed_rate_limits(conn)?;
    Ok(false)
}

/// Walk an existing database forward to [`SCHEMA_USER_VERSION`].
///
/// Returns how many rungs were climbed — zero when the database is already
/// current, which is what a fresh one created from an up-to-date contract is.
pub fn migrate(conn: &mut Connection) -> Result<usize> {
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

    let mut climbed = 0;
    while version < SCHEMA_USER_VERSION {
        let step = MIGRATIONS
            .iter()
            .find(|step| step.from == version)
            .ok_or_else(|| {
                StoreError::conflict(format!("no migration from schema version {version}"))
            })?;
        let tx = conn.transaction()?;
        step.apply(&tx)?;
        tx.commit()?;
        version += 1;
        climbed += 1;
        conn.pragma_update(None, "user_version", version)?;
        tracing::info!(version, "migrated working database");
    }

    conn.pragma_update(None, "user_version", SCHEMA_USER_VERSION)?;
    Ok(climbed)
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

    /// Fabricate a database as it looked at `version`.
    ///
    /// Built by creating the current contract and then walking the ladder
    /// *backwards*, one exact inverse per rung. The old approach edited the DDL
    /// text and broke the moment the contract legitimately grew a column the
    /// ladder also adds; this cannot, because every rung's inverse is defined
    /// next to the rung itself and skips what is already absent.
    ///
    /// A wave-1 database also predates the normative `rate_limits` seeds, so
    /// those are cleared as well — backfilling them is part of that rung.
    fn legacy(conn: &mut Connection, version: i32) {
        assert!(
            (1..=SCHEMA_USER_VERSION).contains(&version),
            "no such historical version: {version}"
        );
        conn.execute_batch(WORKING_DB_SQL).expect("contract ddl");

        for step in MIGRATIONS.iter().rev() {
            if step.from >= version {
                step.revert(conn).expect("revert");
            }
        }
        if version < 2 {
            conn.execute("DELETE FROM rate_limits", []).unwrap();
        }
        conn.pragma_update(None, "user_version", version).unwrap();

        // The fixture is only useful if it really is the older shape.
        for step in MIGRATIONS.iter().filter(|step| step.from >= version) {
            for (table, column, _) in step.add_columns {
                assert!(
                    !columns(conn, table).contains(&(*column).to_string()),
                    "v{version} fixture still carries {table}.{column}"
                );
            }
        }
    }

    fn legacy_v1(conn: &mut Connection) {
        legacy(conn, 1);
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
        assert!(WORKING_DB_SQL.contains("word_count"));
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

    /// Ruling #15, now that the contract carries the column: a fresh database
    /// takes it straight from the DDL and the ladder has nothing left to run.
    #[test]
    fn a_fresh_database_takes_its_columns_from_the_contract() {
        let bare = Connection::open_in_memory().unwrap();
        bare.execute_batch(WORKING_DB_SQL).unwrap();
        assert!(columns(&bare, "releases").contains(&"word_count".to_string()));
        assert!(columns(&bare, "words").contains(&"core_ready".to_string()));

        let mut conn = Connection::open_in_memory().unwrap();
        assert!(ensure_schema(&mut conn).unwrap());
        assert!(columns(&conn, "releases").contains(&"word_count".to_string()));
        assert_eq!(
            migrate(&mut conn).unwrap(),
            0,
            "a database created from an up-to-date contract skips the ladder"
        );
    }

    #[test]
    fn a_wave_two_database_gains_the_release_word_count() {
        let mut conn = Connection::open_in_memory().unwrap();
        legacy(&mut conn, 2);
        assert!(columns(&conn, "words").contains(&"core_ready".to_string()));

        assert!(!ensure_schema(&mut conn).unwrap());

        assert!(columns(&conn, "releases").contains(&"word_count".to_string()));
        let ver: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver, SCHEMA_USER_VERSION);
    }

    /// A rung is a no-op when the column is already there, so a contract sync
    /// that lands ahead of an old database cannot brick the ladder.
    #[test]
    fn a_rung_whose_column_already_exists_is_skipped() {
        let mut conn = Connection::open_in_memory().unwrap();
        // The pathological case: contract DDL (which ships `word_count`)
        // labelled as an older version, so the 2 → 3 rung runs over it.
        conn.execute_batch(WORKING_DB_SQL).unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();

        assert_eq!(migrate(&mut conn).unwrap(), 1, "the rung still runs");
        assert_eq!(
            columns(&conn, "releases")
                .iter()
                .filter(|c| *c == "word_count")
                .count(),
            1
        );
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

        for table in ["words", "releases"] {
            let mut a = columns(&migrated, table);
            let mut b = columns(&fresh, table);
            a.sort();
            b.sort();
            assert_eq!(a, b, "column drift in {table}");
        }
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
        assert_eq!(migrate(&mut conn).unwrap(), 0, "nothing left to climb");
        for (table, column) in [("words", "core_ready"), ("releases", "word_count")] {
            let count = columns(&conn, table)
                .iter()
                .filter(|c| *c == column)
                .count();
            assert_eq!(count, 1, "{table}.{column}");
        }
    }

    /// Every rung must be reachable and reversible, whatever the contract says.
    #[test]
    fn every_historical_version_can_be_fabricated() {
        for version in 1..=SCHEMA_USER_VERSION {
            let mut conn = Connection::open_in_memory().unwrap();
            legacy(&mut conn, version);
            let climbed = migrate(&mut conn).unwrap();
            assert_eq!(
                climbed,
                (SCHEMA_USER_VERSION - version) as usize,
                "wrong number of rungs from v{version}"
            );
            let ver: i32 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(ver, SCHEMA_USER_VERSION);
        }
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
                MIGRATIONS.iter().any(|step| step.from == version),
                "no migration from version {version}"
            );
        }
    }
}
