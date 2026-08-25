//! Schema bootstrap.
//!
//! `docs/contracts/working-db.sql` is the normative DDL and is embedded
//! verbatim — morphod never carries a second copy of the schema. Runtime
//! PRAGMAs come from that file's header comment and live in [`crate::conn`].

use rusqlite::Connection;

use crate::error::Result;
use morpho_domain::version::SCHEMA_USER_VERSION;

/// The normative working-database DDL, embedded at compile time.
pub const WORKING_DB_SQL: &str = include_str!("../../../../docs/contracts/working-db.sql");

/// Default dispatcher lane limits seeded into `rate_limits` on a fresh
/// database. The contract defines the table but ships no rows; these values are
/// morphod's conservative defaults and are editable in the database at runtime.
pub const DEFAULT_RATE_LIMITS: &[(&str, i64, f64, i64)] = &[
    // (rate_key, max_concurrency, refill_per_min, burst)
    ("cpu", 8, 6_000.0, 256),
    ("freedict", 2, 60.0, 10),
    ("wordnet", 4, 6_000.0, 128),
    ("wiktionary", 2, 60.0, 10),
    ("unsplash", 1, 45.0, 5),
    ("pexels", 1, 45.0, 5),
    ("pixabay", 1, 45.0, 5),
    ("sdxl", 1, 6.0, 1),
    ("edge_tts", 3, 120.0, 20),
    ("llm", 2, 30.0, 5),
];

/// Create the schema if the database is empty, then stamp `user_version`.
///
/// Returns `true` when the schema was created by this call.
pub fn ensure_schema(conn: &mut Connection) -> Result<bool> {
    let existing: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let has_tables: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name='words')",
        [],
        |row| row.get::<_, i64>(0).map(|v| v != 0),
    )?;

    if has_tables {
        if existing == 0 {
            // Database predates versioning (or was created by an older build).
            conn.pragma_update(None, "user_version", SCHEMA_USER_VERSION)?;
        }
        seed_rate_limits(conn)?;
        return Ok(false);
    }

    let tx = conn.transaction()?;
    tx.execute_batch(WORKING_DB_SQL)?;
    tx.commit()?;
    conn.pragma_update(None, "user_version", SCHEMA_USER_VERSION)?;
    seed_rate_limits(conn)?;
    Ok(true)
}

/// Insert any missing lane defaults. Existing rows are left untouched so an
/// operator's tuning survives restarts.
pub fn seed_rate_limits(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO rate_limits (rate_key, max_concurrency, refill_per_min, burst)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (rate_key) DO NOTHING",
    )?;
    for (key, concurrency, refill, burst) in DEFAULT_RATE_LIMITS {
        stmt.execute(rusqlite::params![key, concurrency, refill, burst])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_ddl_is_the_contract_file() {
        assert!(WORKING_DB_SQL.contains("CREATE TABLE words ("));
        assert!(WORKING_DB_SQL.contains("CREATE VIEW active_words"));
        assert!(WORKING_DB_SQL.contains("CREATE TABLE release_manifests"));
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
        assert_eq!(count as usize, DEFAULT_RATE_LIMITS.len());
    }
}
