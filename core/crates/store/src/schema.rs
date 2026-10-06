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
    ("wikimedia", 2, 60.0, 4),
    ("openverse", 2, 50.0, 4),
    ("tatoeba", 2, 60.0, 4),
    ("sdxl", 1, 6.0, 1),
    ("clip", 2, 120.0, 8),
    ("codex", 1, 4.0, 1),
    ("edge_tts", 4, 240.0, 8),
    ("llm", 2, 30.0, 4),
    ("cpu", 8, 6000.0, 16),
];

/// Seed values for the required `source` tag category. Mirrors the normative
/// seed rows at the bottom of `docs/contracts/working-db.sql` exactly; applied
/// with `INSERT OR IGNORE` semantics so operator additions survive every
/// restart. Adding a source in production is an admin insert, not an edit here —
/// this list only backfills the vocabulary the shipped datasets already use.
pub const CONTRACT_SOURCE_TAGS: &[&str] = &[
    // Reconciler channels (mirror the image/example `source` columns).
    "unsplash",
    "pexels",
    "pixabay",
    "wikimedia",
    "openverse",
    "sdxl",
    "codex",
    "exam_corpus",
    "freedict",
    "tatoeba",
    "llm",
    // Genuine ad-hoc human upload; never an auto-applied default.
    "manual",
    // Paired image+caption datasets ingested through the manual endpoints.
    "coco",
    "vg",
    "cc3m",
    "wit",
    "commons",
    // Subtitle-mined example sentences (ops/mine_subs.py).
    "opensubtitles",
];

/// A column one migration step introduces: `(table, column, definition)`.
pub type ColumnAdd = (&'static str, &'static str, &'static str);

/// A table one migration step rebuilds, because the change is one `ALTER TABLE`
/// cannot express — SQLite has no way to widen a `CHECK` constraint in place.
///
/// The forward shape normally comes out of the embedded contract, which is the
/// whole point of the file being normative. The *old* shape always needs
/// recording, because nothing else remembers it once the contract moves on, and
/// the test ladder needs it to fabricate a genuine historical database.
#[derive(Debug, Clone, Copy)]
pub struct TableRebuild {
    pub table: &'static str,
    /// The shape this rung produces, for the window where the contract file
    /// legitimately trails the code (see [`CONTRACT_SCHEMA_VERSION`]). `None`
    /// reads it from the contract, which is what a synced rung wants: the
    /// database ends up matching the normative file by construction.
    pub next_ddl: Option<&'static str>,
    /// The table's `CREATE TABLE` statement exactly as it stood before this
    /// rung. Used only by [`Migration::revert`].
    pub previous_ddl: &'static str,
}

/// `example_candidates` before ruling #18 widened its source union.
const EXAMPLE_CANDIDATES_V3: &str = "CREATE TABLE example_candidates (
    ex_cand_id   INTEGER PRIMARY KEY,
    word_id      INTEGER NOT NULL REFERENCES words(word_id),
    text         TEXT NOT NULL,
    text_hash    TEXT NOT NULL,
    hl_start     INTEGER NOT NULL,
    hl_end       INTEGER NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('exam_corpus','llm','manual')),
    source_ref   TEXT,
    status       TEXT NOT NULL DEFAULT 'available' CHECK (status IN ('available','rejected')),
    auto_score   REAL, score_detail TEXT, scorer_ver TEXT,
    created_by   TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (word_id, text_hash)
)";

/// `image_candidates` before ruling #18 widened its source union.
const IMAGE_CANDIDATES_V3: &str = "CREATE TABLE image_candidates (
    img_cand_id  INTEGER PRIMARY KEY,
    word_id      INTEGER NOT NULL REFERENCES words(word_id),
    pos          TEXT,
    file_hash    TEXT NOT NULL REFERENCES media_files(file_hash),
    width        INTEGER, height INTEGER,
    source       TEXT NOT NULL CHECK (source IN ('unsplash','pexels','pixabay','sdxl','manual')),
    source_ref   TEXT,
    license      TEXT,
    query_used   TEXT,
    status       TEXT NOT NULL DEFAULT 'available' CHECK (status IN ('available','rejected')),
    auto_score   REAL, score_detail TEXT, scorer_ver TEXT,
    created_by   TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (word_id, file_hash)
)";

/// `image_candidates` before wave 9 admitted the codex generator.
const IMAGE_CANDIDATES_V6: &str = "CREATE TABLE image_candidates (
    img_cand_id  INTEGER PRIMARY KEY,
    word_id      INTEGER NOT NULL REFERENCES words(word_id),
    pos          TEXT,
    file_hash    TEXT NOT NULL REFERENCES media_files(file_hash),
    width        INTEGER, height INTEGER,
    source       TEXT NOT NULL CHECK (source IN ('unsplash','pexels','pixabay','wikimedia','openverse','sdxl','manual')),
    source_ref   TEXT,
    license      TEXT,
    query_used   TEXT,
    status       TEXT NOT NULL DEFAULT 'available' CHECK (status IN ('available','rejected')),
    auto_score   REAL, score_detail TEXT, scorer_ver TEXT,
    created_by   TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (word_id, file_hash)
)";

/// `oos_queue` as the contract still spells it: before ruling #18a gave the
/// queue a third way to close a lemma.
const OOS_QUEUE_V5: &str = "CREATE TABLE oos_queue (
    oos_lemma  TEXT PRIMARY KEY COLLATE NOCASE,
    status     TEXT NOT NULL DEFAULT 'open'
               CHECK (status IN ('open','resolved_rewrite','resolved_promote','auto_closed')),
    first_seen TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    resolved_by TEXT, resolved_at TEXT, notes TEXT
)";

impl TableRebuild {
    /// The shape this rung is aiming at, plus the table's indexes.
    ///
    /// The indexes are the contract's either way: no rung so far has changed
    /// one, and a rung that did would have to carry them alongside its DDL.
    fn wanted(&self) -> Result<ContractDdl> {
        let contract = contract_ddl(self.table)?;
        Ok(match self.next_ddl {
            None => contract,
            Some(create) => ContractDdl {
                create: create.to_string(),
                indexes: contract.indexes,
            },
        })
    }
}

/// One rung of the migration ladder: a database at `from` becomes `from + 1`.
///
/// A rung is data, not a SQL string, and the type enumerates exactly the kinds
/// of change the ladder knows how to make. That buys two properties a bag of
/// statements cannot:
///
/// * **idempotence** — [`Migration::apply`] checks before it changes anything,
///   so a contract file that has already caught up (the DDL ships the column or
///   the widened `CHECK`, an old database does not) can never brick the ladder;
/// * **an exact inverse** — [`Migration::revert`] lets the tests fabricate a
///   genuine historical schema by walking the current contract *backwards*,
///   instead of doing string surgery on a file that legitimately evolves.
///
/// A future rung that needs more than these adds a field here and its own
/// inverse alongside it. It does not get to smuggle DDL past the guard.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub from: i32,
    pub add_columns: &'static [ColumnAdd],
    pub rebuilds: &'static [TableRebuild],
    /// Tables this rung introduces, named rather than spelled out: the DDL and
    /// the indexes come from the embedded contract, which is the whole point of
    /// that file being normative. A wholly new table needs no data migration —
    /// there is nothing to carry across — so "create it if it is not there" is
    /// the entire forward step, and dropping it is the exact inverse.
    pub creates: &'static [&'static str],
    /// Views this rung redefines. The new text comes from the contract; a view
    /// is pure metadata, so replacing one is a drop and a create.
    pub views: &'static [ViewReplace],
}

/// One view a rung redefines.
#[derive(Debug, Clone, Copy)]
pub struct ViewReplace {
    pub view: &'static str,
    /// The view's `CREATE VIEW` statement exactly as it stood before this rung.
    /// Used only by [`Migration::revert`].
    pub previous_ddl: &'static str,
}

/// `aux_liveness` before liveness became reachability from the targets: any
/// reference counted, including one from a retired word.
const AUX_LIVENESS_V8: &str = "CREATE VIEW aux_liveness AS
SELECT w.word_id,
       EXISTS (SELECT 1 FROM def_dependencies d WHERE d.depends_on_word_id = w.word_id)
    OR EXISTS (SELECT 1 FROM distractors x WHERE x.distractor_word_id = w.word_id)
       AS is_live
FROM words w WHERE w.role = 'auxiliary'";

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        from: 1,
        // Wave 2: the reconciler owns a materialized `core_ready` so
        // `DistractorView.core_ready` is a column read rather than a derivation
        // hack (admin-api.md wave-2 ruling #5).
        add_columns: &[("words", "core_ready", "INTEGER NOT NULL DEFAULT 0")],
        rebuilds: &[],
        creates: &[],
        views: &[],
    },
    Migration {
        from: 2,
        // Wave 3: the release history owns its word count, so `GET /releases`
        // reads a column instead of re-parsing the audit log
        // (admin-api.md wave-3 ruling #15). Releases exported before this
        // migration keep the 0 default; their count only ever lived in the
        // event detail.
        add_columns: &[("releases", "word_count", "INTEGER NOT NULL DEFAULT 0")],
        rebuilds: &[],
        creates: &[],
        views: &[],
    },
    Migration {
        from: 3,
        // Wave 4: ruling #18 adds the keyless sources, which widens two `CHECK`
        // unions. SQLite cannot alter a constraint, so both tables are rebuilt.
        // The three new `rate_limits` lanes need no rung of their own —
        // `seed_rate_limits` runs `INSERT OR IGNORE` on every boot.
        add_columns: &[],
        rebuilds: &[
            TableRebuild {
                table: "example_candidates",
                next_ddl: None,
                previous_ddl: EXAMPLE_CANDIDATES_V3,
            },
            TableRebuild {
                table: "image_candidates",
                next_ddl: None,
                previous_ddl: IMAGE_CANDIDATES_V3,
            },
        ],
        creates: &[],
        views: &[],
    },
    Migration {
        from: 4,
        // Wave 7: the gloss anchor (admin-api.md ruling #18a). A word that is
        // referenced by definitions but is never going to be learnable carries
        // a short Chinese gloss, and that gloss terminates the readability
        // chain exactly like a base word.
        add_columns: &[
            ("words", "zh_gloss", "TEXT"),
            (
                "words",
                "zh_gloss_source",
                "TEXT CHECK (zh_gloss_source IN ('manual','cedict'))",
            ),
        ],
        rebuilds: &[],
        creates: &[],
        views: &[],
    },
    Migration {
        from: 5,
        // Wave 7, the other half: anchoring a lemma is a third way to close an
        // out-of-scope queue entry, which widens a `CHECK` and therefore needs
        // its own rebuild rung.
        add_columns: &[],
        rebuilds: &[TableRebuild {
            table: "oos_queue",
            next_ddl: None,
            previous_ddl: OOS_QUEUE_V5,
        }],
        creates: &[],
        views: &[],
    },
    Migration {
        from: 6,
        // Wave 9: image selection learns what a picture *means*. `clip_scores`
        // is a wholly new table, so it is created from the contract rather than
        // migrated into; the `image_candidates` union widens for the codex
        // generator, which SQLite can only express as a rebuild.
        add_columns: &[],
        rebuilds: &[TableRebuild {
            table: "image_candidates",
            next_ddl: None,
            previous_ddl: IMAGE_CANDIDATES_V6,
        }],
        creates: &["clip_scores"],
        views: &[],
    },
    Migration {
        from: 7,
        // #54: the candidate tagging tables. Data-driven provenance replacing the
        // overloaded `source = 'manual'`. Three wholly new tables created from the
        // contract; the `source` columns and their CHECK unions are untouched, so
        // no candidate table is rebuilt and nothing here can move a selection. The
        // seed vocabulary is backfilled by `seed_candidate_tags` on every boot,
        // exactly as the `rate_limits` lanes are.
        add_columns: &[],
        rebuilds: &[],
        creates: &["tag_category", "tag", "candidate_tag"],
        views: &[],
    },
    Migration {
        from: 8,
        // Auxiliary liveness becomes reachability from the targets. The old view
        // counted any reference, so a retired auxiliary's definitions, or a cycle
        // of auxiliaries, kept words alive that nothing shipped needed.
        add_columns: &[],
        rebuilds: &[],
        creates: &[],
        views: &[ViewReplace {
            view: "aux_liveness",
            previous_ddl: AUX_LIVENESS_V8,
        }],
    },
];

/// The contract file may trail the code, never lead it: a DDL from the future
/// would be created and then have migrations replayed on top of it.
const _: () = assert!(CONTRACT_SCHEMA_VERSION <= SCHEMA_USER_VERSION);

impl Migration {
    /// Apply this rung. Changes that are already present are skipped, so the
    /// step is safe on a database created from a contract file that ships them.
    fn apply(&self, tx: &rusqlite::Transaction<'_>) -> Result<()> {
        for table in self.creates {
            if table_exists(tx, table)? {
                tracing::debug!(table, "table already present; creation skipped");
                continue;
            }
            let want = contract_ddl(table)?;
            tx.execute_batch(&want.create)?;
            for index in &want.indexes {
                tx.execute_batch(index)?;
            }
        }
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
        for rebuild in self.rebuilds {
            let want = rebuild.wanted()?;
            if same_shape(&live_ddl(tx, rebuild.table)?, &want.create) {
                tracing::debug!(
                    table = rebuild.table,
                    "table already has the contract shape; rebuild skipped"
                );
                continue;
            }
            rebuild_table(tx, rebuild.table, &want.create, &want.indexes)?;
        }
        for replace in self.views {
            let want = contract_view(replace.view)?;
            if same_shape(&live_view(tx, replace.view)?, &want) {
                tracing::debug!(view = replace.view, "view already has the contract shape; skipped");
                continue;
            }
            tx.execute_batch(&format!("DROP VIEW IF EXISTS \"{}\"", replace.view))?;
            tx.execute_batch(&want)?;
        }
        Ok(())
    }

    /// Does this rung need foreign keys switched off around it?
    ///
    /// A rebuild drops and recreates a table other tables point at, so the
    /// answer is yes exactly when it rebuilds something (SQLite's own 12-step
    /// procedure, steps 1 and 12).
    const fn touches_foreign_keys(&self) -> bool {
        !self.rebuilds.is_empty()
    }

    /// Undo this rung, so tests can fabricate the schema that preceded it.
    ///
    /// Lives next to [`Migration::apply`] on purpose: a new rung that forgets
    /// its inverse is a compile-time impossibility rather than a fixture that
    /// silently rots.
    #[cfg(test)]
    fn revert(&self, tx: &rusqlite::Transaction<'_>) -> Result<()> {
        for rebuild in self.rebuilds {
            if same_shape(&live_ddl(tx, rebuild.table)?, rebuild.previous_ddl) {
                continue;
            }
            let indexes = contract_ddl(rebuild.table)?.indexes;
            rebuild_table(tx, rebuild.table, rebuild.previous_ddl, &indexes)?;
        }
        for (table, column, _) in self.add_columns {
            if !has_column(tx, table, column)? {
                continue;
            }
            tx.execute_batch(&format!("ALTER TABLE {table} DROP COLUMN {column}"))?;
        }
        for table in self.creates {
            tx.execute_batch(&format!("DROP TABLE IF EXISTS \"{table}\""))?;
        }
        for replace in self.views {
            tx.execute_batch(&format!("DROP VIEW IF EXISTS \"{}\"", replace.view))?;
            tx.execute_batch(replace.previous_ddl)?;
        }
        Ok(())
    }
}

/// Does this database have `table` at all?
fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        rusqlite::params![table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
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

// ---------------------------------------------------------------------------
// Table rebuilds (SQLite's 12-step "other kinds of schema change" procedure)
// ---------------------------------------------------------------------------

/// One table's DDL as the contract states it.
struct ContractDdl {
    create: String,
    indexes: Vec<String>,
}

/// Drop `--` comments. The contract has none inside a string literal, and it
/// does carry a `;` inside one, so this runs before any statement splitting.
fn strip_comments(sql: &str) -> String {
    sql.lines()
        .map(|line| line.split_once("--").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Split a SQL script into statements.
fn statements(sql: &str) -> Vec<String> {
    strip_comments(sql)
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
        .map(str::to_string)
        .collect()
}

/// Two DDL texts describe the same object, ignoring layout, comments and
/// quoting.
///
/// All three genuinely differ between the two sides being compared: SQLite
/// rewrites the stored `CREATE TABLE` text when a table is renamed and quotes
/// the new name, and the text a rebuild feeds it has already had the contract's
/// comments stripped. Normalizing is what makes "has this rebuild already
/// happened?" a reliable question.
fn same_shape(a: &str, b: &str) -> bool {
    fn normalize(sql: &str) -> String {
        strip_comments(sql)
            .replace(['"', '`'], "")
            .replace('(', " ( ")
            .replace(')', " ) ")
            .replace(',', " , ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
    normalize(a) == normalize(b)
}

fn is_create_table(statement: &str, table: &str) -> bool {
    statement
        .strip_prefix("CREATE TABLE ")
        .and_then(|rest| rest.trim_start().strip_prefix(table))
        .is_some_and(|rest| rest.trim_start().starts_with('('))
}

fn is_index_on(statement: &str, table: &str) -> bool {
    if !statement.starts_with("CREATE INDEX") && !statement.starts_with("CREATE UNIQUE INDEX") {
        return false;
    }
    statement
        .split_once(" ON ")
        .and_then(|(_, rest)| rest.trim_start().strip_prefix(table))
        .is_some_and(|rest| rest.trim_start().starts_with('('))
}

/// The contract's shape for one table: its `CREATE TABLE` plus its indexes.
fn contract_ddl(table: &str) -> Result<ContractDdl> {
    let all = statements(WORKING_DB_SQL);
    let create = all
        .iter()
        .find(|statement| is_create_table(statement, table))
        .cloned()
        .ok_or_else(|| {
            StoreError::conflict(format!("the contract has no CREATE TABLE for {table}"))
        })?;
    let indexes = all
        .into_iter()
        .filter(|statement| is_index_on(statement, table))
        .collect();
    Ok(ContractDdl { create, indexes })
}

/// The contract's `CREATE VIEW` statement for `view`.
fn contract_view(view: &str) -> Result<String> {
    statements(WORKING_DB_SQL)
        .into_iter()
        .find(|statement| {
            statement
                .strip_prefix("CREATE VIEW ")
                .and_then(|rest| rest.trim_start().strip_prefix(view))
                .is_some_and(|rest| rest.trim_start().starts_with("AS"))
        })
        .ok_or_else(|| StoreError::conflict(format!("the contract has no CREATE VIEW for {view}")))
}

/// The `CREATE VIEW` text this database actually holds for `view` (empty when
/// it has none).
fn live_view(conn: &Connection, view: &str) -> Result<String> {
    let sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'view' AND name = ?1",
            rusqlite::params![view],
            |row| row.get(0),
        )
        .ok();
    Ok(sql.unwrap_or_default())
}

/// The `CREATE TABLE` text this database actually holds for `table`.
fn live_ddl(conn: &Connection, table: &str) -> Result<String> {
    let sql: Option<String> = conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
        rusqlite::params![table],
        |row| row.get(0),
    )?;
    sql.ok_or_else(|| StoreError::conflict(format!("table {table} does not exist")))
}

/// Replace one table with a new definition, carrying every row across.
///
/// This is the 12-step procedure from SQLite's `ALTER TABLE` documentation.
/// Steps 1 and 12 (the `foreign_keys` pragma) belong to the caller, because a
/// pragma cannot be changed inside a transaction; [`migrate`] does them.
///
/// Every view is dropped and recreated around the swap. Step 7's
/// `ALTER TABLE … RENAME TO` reparses the whole schema and fails if any view
/// still names the table that step 6 dropped, and views are pure metadata, so
/// dropping all of them is both necessary and free.
fn rebuild_table(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    create: &str,
    indexes: &[String],
) -> Result<()> {
    let staging = format!("{table}_morpho_rebuild");

    // Step 3: remember every view, in creation order — a view may select from
    // another one, so the order it is put back in matters.
    let views: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT name, sql FROM sqlite_master
             WHERE type = 'view' AND sql IS NOT NULL ORDER BY rowid",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (name, _) in &rows {
            tx.execute_batch(&format!("DROP VIEW IF EXISTS \"{name}\""))?;
        }
        rows.into_iter().map(|(_, sql)| sql).collect()
    };

    // Step 4: the new table, under a name nothing else refers to.
    let body = create
        .strip_prefix("CREATE TABLE ")
        .and_then(|rest| rest.trim_start().strip_prefix(table))
        .ok_or_else(|| {
            StoreError::conflict(format!("rebuild DDL for {table} is not its CREATE TABLE"))
        })?;
    tx.execute_batch(&format!("CREATE TABLE \"{staging}\"{body}"))?;

    // Step 5: carry the rows over by name, so a rebuild that also adds or drops
    // a column moves what the two shapes have in common and nothing else.
    let shared = shared_columns(tx, table, &staging)?;
    if shared.is_empty() {
        return Err(StoreError::conflict(format!(
            "the old and new {table} share no columns"
        )));
    }
    let columns = shared
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    tx.execute_batch(&format!(
        "INSERT INTO \"{staging}\" ({columns}) SELECT {columns} FROM \"{table}\""
    ))?;

    // Steps 6 and 7.
    tx.execute_batch(&format!("DROP TABLE \"{table}\""))?;
    tx.execute_batch(&format!("ALTER TABLE \"{staging}\" RENAME TO \"{table}\""))?;

    // Step 8: the table's own indexes.
    for index in indexes {
        tx.execute_batch(index)?;
    }
    // Step 9: the views, back in the order they were created.
    for view in &views {
        tx.execute_batch(view)?;
    }
    Ok(())
}

/// Column names both tables have, in the *new* table's order.
fn shared_columns(conn: &Connection, old: &str, new: &str) -> Result<Vec<String>> {
    let names = |table: &str| -> Result<Vec<String>> {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")?;
        let rows = stmt
            .query_map(rusqlite::params![table], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    };
    let existing: std::collections::HashSet<String> = names(old)?.into_iter().collect();
    Ok(names(new)?
        .into_iter()
        .filter(|name| existing.contains(name))
        .collect())
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
        seed_candidate_tags(conn)?;
        return Ok(true);
    }

    migrate(conn)?;
    seed_rate_limits(conn)?;
    seed_candidate_tags(conn)?;
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
        run_step(conn, step)?;
        version += 1;
        climbed += 1;
        conn.pragma_update(None, "user_version", version)?;
        tracing::info!(version, "migrated working database");
    }

    conn.pragma_update(None, "user_version", SCHEMA_USER_VERSION)?;
    Ok(climbed)
}

/// Run one rung inside its own transaction.
///
/// A rung that rebuilds a table needs `foreign_keys` off while the table is
/// briefly absent, and a pragma cannot be changed inside a transaction — so the
/// switch lives out here, and the integrity check that justifies it runs before
/// the commit. The original pragma value is restored whether the rung succeeded
/// or not.
fn run_step(conn: &mut Connection, step: &Migration) -> Result<()> {
    let guard_foreign_keys = step.touches_foreign_keys();
    let were_on: bool = conn.query_row("PRAGMA foreign_keys", [], |row| {
        row.get::<_, i64>(0).map(|value| value != 0)
    })?;
    if guard_foreign_keys && were_on {
        conn.pragma_update(None, "foreign_keys", false)?;
    }

    let outcome = (|| -> Result<()> {
        let tx = conn.transaction()?;
        step.apply(&tx)?;
        if guard_foreign_keys && were_on {
            check_foreign_keys(&tx)?;
        }
        tx.commit()?;
        Ok(())
    })();

    if guard_foreign_keys && were_on {
        conn.pragma_update(None, "foreign_keys", true)?;
    }
    outcome
}

/// Step 10: refuse to commit a rebuild that orphaned a reference.
fn check_foreign_keys(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("SELECT \"table\", rowid FROM pragma_foreign_key_check")?;
    let violations = stmt
        .query_map([], |row| {
            Ok(format!(
                "{}#{:?}",
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if violations.is_empty() {
        return Ok(());
    }
    Err(StoreError::conflict(format!(
        "migration left {} dangling reference(s): {}",
        violations.len(),
        violations.join(", ")
    )))
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

/// Insert any missing candidate-tag vocabulary. Mirrors the normative seed rows
/// at the bottom of `docs/contracts/working-db.sql` so a database *migrated*
/// into schema 8 (whose `creates` step makes the tables but not their seed
/// rows) ends up with the same vocabulary a freshly created one has. Existing
/// rows and operator additions are left untouched.
pub fn seed_candidate_tags(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO tag_category (category, required, exclusive, note)
         VALUES ('source', 1, 1,
                 'Authoritative provenance; exactly one required per candidate.')
         ON CONFLICT (category) DO NOTHING",
        [],
    )?;
    let mut stmt = conn.prepare(
        "INSERT INTO tag (category, value) VALUES ('source', ?1)
         ON CONFLICT (category, value) DO NOTHING",
    )?;
    for value in CONTRACT_SOURCE_TAGS {
        stmt.execute(rusqlite::params![value])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fabricate a database as it looked at `version`.
    ///
    /// Built by creating the current contract — which describes
    /// [`CONTRACT_SCHEMA_VERSION`] — and then walking the ladder to the wanted
    /// rung: *backwards* for an older shape, one exact inverse per rung, and
    /// forwards through the window where the contract file trails the code. An
    /// earlier version of this helper edited the DDL text and broke the moment
    /// the contract legitimately grew a column the ladder also adds; this
    /// cannot, because every rung's inverse is defined next to the rung itself
    /// and skips what is already absent.
    ///
    /// A wave-1 database also predates the normative `rate_limits` seeds, so
    /// those are cleared as well — backfilling them is part of that rung.
    fn legacy(conn: &mut Connection, version: i32) {
        assert!(
            (1..=SCHEMA_USER_VERSION).contains(&version),
            "no such historical version: {version}"
        );
        conn.execute_batch(WORKING_DB_SQL).expect("contract ddl");

        conn.pragma_update(None, "foreign_keys", false).unwrap();
        for step in MIGRATIONS
            .iter()
            .filter(|step| step.from >= CONTRACT_SCHEMA_VERSION && step.from < version)
        {
            let tx = conn.transaction().unwrap();
            step.apply(&tx).expect("apply");
            tx.commit().unwrap();
        }
        for step in MIGRATIONS.iter().rev() {
            if step.from >= version {
                let tx = conn.transaction().unwrap();
                step.revert(&tx).expect("revert");
                tx.commit().unwrap();
            }
        }
        conn.pragma_update(None, "foreign_keys", true).unwrap();

        // Lane seeds arrived with rung 1→2 and grew again with 3→4; a fixture
        // that already carried them would prove nothing about the backfill.
        if version < 2 {
            conn.execute("DELETE FROM rate_limits", []).unwrap();
        } else if version < 4 {
            conn.execute(
                "DELETE FROM rate_limits WHERE rate_key IN ('wikimedia','openverse','tatoeba')",
                [],
            )
            .unwrap();
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
            for rebuild in step.rebuilds {
                // A table can be rebuilt by more than one rung
                // (`image_candidates` widened at 3→4 and again at 6→7), and the
                // fixture carries the shape the *earliest* pending rung starts
                // from — every later one is still ahead of it.
                let earliest = MIGRATIONS
                    .iter()
                    .filter(|other| other.from >= version)
                    .flat_map(|other| other.rebuilds)
                    .find(|other| other.table == rebuild.table)
                    .expect("this rung is one of them");
                assert!(
                    same_shape(
                        &live_ddl(conn, rebuild.table).unwrap(),
                        earliest.previous_ddl
                    ),
                    "v{version} fixture does not carry the expected {} shape",
                    rebuild.table
                );
            }
            for table in step.creates {
                assert!(
                    !table_exists(conn, table).unwrap(),
                    "v{version} fixture already carries {table}"
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

    /// Insert one candidate row of each family, so a rebuild has something to
    /// carry across.
    fn seed_candidates(conn: &Connection) -> (i64, i64) {
        conn.execute(
            "INSERT INTO words (word_id, lemma, role, blockers) VALUES (7,'serene','target','[]')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO media_files (file_hash, kind, rel_path, bytes)
             VALUES ('abc','image','ab/abc.webp', 10)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO example_candidates
                 (ex_cand_id, word_id, text, text_hash, hl_start, hl_end, source, created_by)
             VALUES (11, 7, 'A serene lake.', 'h1', 2, 8, 'exam_corpus', 'cli')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO image_candidates
                 (img_cand_id, word_id, file_hash, source, created_by)
             VALUES (21, 7, 'abc', 'unsplash', 'cli')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO example_selections (word_id, slot, ex_cand_id, selected_by)
             VALUES (7, 1, 11, 'auto')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO image_selections (word_id, img_cand_id, selected_by)
             VALUES (7, 21, 'auto')",
            [],
        )
        .unwrap();
        (11, 21)
    }

    /// Can this database hold a candidate with that source?
    fn accepts_source(conn: &Connection, table: &str, source: &str) -> bool {
        let sql = if table == "example_candidates" {
            format!(
                "INSERT INTO example_candidates
                     (word_id, text, text_hash, hl_start, hl_end, source, created_by)
                 VALUES (7, 'A probe of {source}.', 'probe-{source}', 2, 7, '{source}', 'cli')"
            )
        } else {
            // The candidate references its bytes, so the probe registers them.
            conn.execute(
                "INSERT OR IGNORE INTO media_files (file_hash, kind, rel_path, bytes)
                 VALUES (?1, 'image', 'pr/probe.webp', 1)",
                rusqlite::params![format!("probe-{source}")],
            )
            .expect("probe media row");
            format!(
                "INSERT INTO image_candidates (word_id, file_hash, source, created_by)
                 VALUES (7, 'probe-{source}', '{source}', 'cli')"
            )
        };
        conn.execute(&sql, []).is_ok()
    }

    #[test]
    fn embedded_ddl_is_the_contract_file() {
        assert!(WORKING_DB_SQL.contains("CREATE TABLE words ("));
        assert!(WORKING_DB_SQL.contains("CREATE VIEW active_words"));
        assert!(WORKING_DB_SQL.contains("CREATE TABLE release_manifests"));
        assert!(WORKING_DB_SQL.contains("core_ready"));
        assert!(WORKING_DB_SQL.contains("word_count"));
        assert!(WORKING_DB_SQL.contains("zh_gloss"));
        assert!(WORKING_DB_SQL.contains("CREATE TABLE clip_scores"));
        assert!(WORKING_DB_SQL.contains("CREATE TABLE tag_category"));
        assert!(WORKING_DB_SQL.contains("CREATE TABLE candidate_tag"));
    }

    /// The typed enumerations and the SQL `CHECK` unions are two spellings of
    /// one vocabulary; a value that round-trips through the type system must be
    /// insertable.
    #[test]
    fn the_source_enumerations_match_the_contract_check_constraints() {
        use morpho_domain::types::{ExampleSource, ImageSource};

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(WORKING_DB_SQL).unwrap();
        seed_candidates(&conn);
        for source in ExampleSource::ALL {
            assert!(
                accepts_source(&conn, "example_candidates", source.as_str()),
                "example source {source} is not in the contract CHECK"
            );
        }
        for source in ImageSource::ALL {
            assert!(
                accepts_source(&conn, "image_candidates", source.as_str()),
                "image source {source} is not in the contract CHECK"
            );
        }
        assert!(!accepts_source(&conn, "image_candidates", "getty"));
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
        assert!(columns(&bare, "words").contains(&"zh_gloss".to_string()));

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

    /// Ruling #18, the rung that cannot be an `ALTER TABLE`: a v3 database
    /// rejects the keyless sources, and after the ladder it accepts them —
    /// with every row, id and selection still in place.
    #[test]
    fn a_wave_three_database_gains_the_keyless_sources() {
        let mut conn = Connection::open_in_memory().unwrap();
        legacy(&mut conn, 3);
        seed_candidates(&conn);
        assert!(!accepts_source(&conn, "example_candidates", "tatoeba"));
        assert!(!accepts_source(&conn, "image_candidates", "wikimedia"));

        assert!(!ensure_schema(&mut conn).unwrap());

        for source in ["freedict", "tatoeba"] {
            assert!(
                accepts_source(&conn, "example_candidates", source),
                "{source}"
            );
        }
        for source in ["wikimedia", "openverse"] {
            assert!(
                accepts_source(&conn, "image_candidates", source),
                "{source}"
            );
        }

        // The rows survived the rebuild, keeping the ids their selections point at.
        let (text, hl): (String, i64) = conn
            .query_row(
                "SELECT text, hl_start FROM example_candidates WHERE ex_cand_id = 11",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((text.as_str(), hl), ("A serene lake.", 2));
        let selected: i64 = conn
            .query_row(
                "SELECT es.ex_cand_id FROM example_selections es
                 JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
                 WHERE es.word_id = 7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(selected, 11);
        let image: String = conn
            .query_row(
                "SELECT source FROM image_candidates WHERE img_cand_id = 21",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(image, "unsplash");
    }

    /// Can this database close an out-of-scope lemma with that status?
    fn accepts_oos_status(conn: &Connection, status: &str) -> bool {
        conn.execute(
            "INSERT INTO oos_queue (oos_lemma, status) VALUES (?1, ?2)",
            rusqlite::params![format!("probe-{status}"), status],
        )
        .is_ok()
    }

    /// Ruling #18a, first rung: a wave-4 database gains the gloss columns, and
    /// the rows it already had come through with them empty.
    #[test]
    fn a_wave_four_database_gains_the_gloss_columns() {
        let mut conn = Connection::open_in_memory().unwrap();
        legacy(&mut conn, 4);
        assert!(!columns(&conn, "words").contains(&"zh_gloss".to_string()));
        conn.execute(
            "INSERT INTO words (word_id, lemma, role, blockers) VALUES (7,'serene','target','[]')",
            [],
        )
        .unwrap();

        assert!(!ensure_schema(&mut conn).unwrap());

        for column in ["zh_gloss", "zh_gloss_source"] {
            assert!(
                columns(&conn, "words").contains(&column.to_string()),
                "{column}"
            );
        }
        let (lemma, gloss): (String, Option<String>) = conn
            .query_row("SELECT lemma, zh_gloss FROM words", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((lemma.as_str(), gloss), ("serene", None));

        // The source column carries the contract's CHECK, not just its type.
        conn.execute(
            "UPDATE words SET zh_gloss = '宁静的', zh_gloss_source = 'manual'",
            [],
        )
        .unwrap();
        assert!(conn
            .execute("UPDATE words SET zh_gloss_source = 'guesswork'", [])
            .is_err());
        // And `active_words` re-expands to see it.
        let through_view: Option<String> = conn
            .query_row("SELECT zh_gloss FROM active_words", [], |r| r.get(0))
            .unwrap();
        assert_eq!(through_view.as_deref(), Some("宁静的"));
    }

    /// Ruling #18a, second rung: the `oos_queue.status` CHECK widens. The
    /// contract file has not caught up, so this is the one rung whose forward
    /// shape lives in the code — and it must still run over a database created
    /// straight from that file.
    #[test]
    fn a_wave_five_database_gains_the_gloss_resolution() {
        let mut conn = Connection::open_in_memory().unwrap();
        legacy(&mut conn, 5);
        assert!(columns(&conn, "words").contains(&"zh_gloss".to_string()));
        conn.execute(
            "INSERT INTO oos_queue (oos_lemma, status) VALUES ('perceive','open')",
            [],
        )
        .unwrap();
        assert!(!accepts_oos_status(&conn, "resolved_gloss"));

        assert!(!ensure_schema(&mut conn).unwrap());

        assert!(accepts_oos_status(&conn, "resolved_gloss"));
        assert!(accepts_oos_status(&conn, "resolved_promote"));
        assert!(!accepts_oos_status(&conn, "resolved_telepathy"));
        // The rebuild carried the queue across.
        let status: String = conn
            .query_row(
                "SELECT status FROM oos_queue WHERE oos_lemma = 'perceive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "open");
    }

    /// Wave 9: a v6 database gains `clip_scores` and the codex source, and the
    /// image candidates it already held come through the rebuild untouched.
    #[test]
    fn a_wave_six_database_gains_clip_scores_and_the_codex_source() {
        let mut conn = Connection::open_in_memory().unwrap();
        legacy(&mut conn, 6);
        seed_candidates(&conn);
        assert!(!table_exists(&conn, "clip_scores").unwrap());
        assert!(!accepts_source(&conn, "image_candidates", "codex"));

        assert!(!ensure_schema(&mut conn).unwrap());

        assert!(table_exists(&conn, "clip_scores").unwrap());
        assert!(accepts_source(&conn, "image_candidates", "codex"));
        // The pictures the library already held survived, keeping the ids their
        // selections point at.
        let (source, selected): (String, i64) = conn
            .query_row(
                "SELECT c.source, s.img_cand_id FROM image_selections s
                 JOIN image_candidates c ON c.img_cand_id = s.img_cand_id
                 WHERE s.word_id = 7",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((source.as_str(), selected), ("unsplash", 21));

        // The new table is a real one, with its key and its foreign key live.
        conn.execute(
            "INSERT INTO clip_scores (file_hash, text_hash, model_ver, similarity)
             VALUES ('abc', 'q1', 'clip/1:ViT-B-32', 0.27)",
            [],
        )
        .unwrap();
        assert!(conn
            .execute(
                "INSERT INTO clip_scores (file_hash, text_hash, model_ver, similarity)
                 VALUES ('abc', 'q1', 'clip/1:ViT-B-32', 0.31)",
                [],
            )
            .is_err());
        // A different model is a different row, never an overwrite.
        conn.execute(
            "INSERT INTO clip_scores (file_hash, text_hash, model_ver, similarity)
             VALUES ('abc', 'q1', 'clip/2:ViT-L-14', 0.31)",
            [],
        )
        .unwrap();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM clip_scores", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 2);
    }

    /// #54: a v7 database gains the three candidate-tag tables and the seed
    /// vocabulary, and the candidates it already held survive untouched — the
    /// rung creates tables, it rebuilds nothing.
    #[test]
    fn a_wave_seven_database_gains_the_candidate_tag_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        legacy(&mut conn, 7);
        seed_candidates(&conn);
        for table in ["tag_category", "tag", "candidate_tag"] {
            assert!(
                !table_exists(&conn, table).unwrap(),
                "{table} present at v7"
            );
        }
        let before = live_ddl(&conn, "image_candidates").unwrap();

        assert!(!ensure_schema(&mut conn).unwrap());

        for table in ["tag_category", "tag", "candidate_tag"] {
            assert!(
                table_exists(&conn, table).unwrap(),
                "{table} missing after migrate"
            );
        }
        // Purely additive: the candidate tables are not rebuilt.
        assert_eq!(live_ddl(&conn, "image_candidates").unwrap(), before);
        let (source, selected): (String, i64) = conn
            .query_row(
                "SELECT c.source, s.img_cand_id FROM image_selections s
                 JOIN image_candidates c ON c.img_cand_id = s.img_cand_id WHERE s.word_id = 7",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((source.as_str(), selected), ("unsplash", 21));

        // The seed vocabulary is present, and 'source' is required + exclusive.
        let (required, exclusive): (i64, i64) = conn
            .query_row(
                "SELECT required, exclusive FROM tag_category WHERE category = 'source'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((required, exclusive), (1, 1));
        let sources: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tag WHERE category = 'source'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sources as usize, CONTRACT_SOURCE_TAGS.len());

        // The FK/uniqueness guarantees below need foreign keys enforced, which a
        // raw test connection does not do by default (morphod sets it at runtime).
        conn.pragma_update(None, "foreign_keys", true).unwrap();

        // A referenced tag cannot be deleted; an unreferenced one can.
        let vg: i64 = conn
            .query_row(
                "SELECT tag_id FROM tag WHERE category='source' AND value='vg'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO candidate_tag (entity_type, cand_id, tag_id, category, assigned_by)
             VALUES ('image', 21, ?1, 'source', 'test')",
            rusqlite::params![vg],
        )
        .unwrap();
        assert!(
            conn.execute("DELETE FROM tag WHERE tag_id = ?1", rusqlite::params![vg])
                .is_err(),
            "a referenced source tag must not be deletable"
        );
        // Exactly one tag per (candidate, category): a second source is refused.
        let coco: i64 = conn
            .query_row(
                "SELECT tag_id FROM tag WHERE category='source' AND value='coco'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            conn.execute(
                "INSERT INTO candidate_tag (entity_type, cand_id, tag_id, category, assigned_by)
                 VALUES ('image', 21, ?1, 'source', 'test')",
                rusqlite::params![coco],
            )
            .is_err(),
            "a second source tag on one candidate must be refused"
        );
        // The denormalized category is pinned to the tag's real category.
        assert!(
            conn.execute(
                "INSERT INTO candidate_tag (entity_type, cand_id, tag_id, category, assigned_by)
                 VALUES ('example', 11, ?1, 'quality', 'test')",
                rusqlite::params![coco],
            )
            .is_err(),
            "a mismatched (tag_id, category) must be refused by the composite FK"
        );
    }

    /// The rebuild puts back everything it took apart.
    #[test]
    fn a_rebuild_restores_the_indexes_and_the_views() {
        let mut migrated = Connection::open_in_memory().unwrap();
        legacy(&mut migrated, 3);
        ensure_schema(&mut migrated).unwrap();

        let objects = |conn: &Connection, kind: &str| -> Vec<(String, String)> {
            let mut stmt = conn
                .prepare(
                    "SELECT name, COALESCE(sql, '') FROM sqlite_master
                     WHERE type = ?1 ORDER BY name",
                )
                .unwrap();
            stmt.query_map(rusqlite::params![kind], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
        };

        let mut fresh = Connection::open_in_memory().unwrap();
        ensure_schema(&mut fresh).unwrap();

        for kind in ["index", "view"] {
            let a = objects(&migrated, kind);
            let b = objects(&fresh, kind);
            assert_eq!(
                a.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                b.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                "{kind} set drift"
            );
            for ((name, migrated_sql), (_, fresh_sql)) in a.iter().zip(b.iter()) {
                assert!(
                    same_shape(migrated_sql, fresh_sql),
                    "{kind} {name} drifted:\n{migrated_sql}\n{fresh_sql}"
                );
            }
        }
        // And the view really resolves against the rebuilt table.
        let desired: i64 = migrated
            .query_row("SELECT COUNT(*) FROM tts_desired", [], |r| r.get(0))
            .unwrap();
        assert_eq!(desired, 0);
    }

    /// A rebuild refuses to commit a schema whose references no longer resolve.
    #[test]
    fn a_rebuild_that_orphans_a_reference_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        legacy(&mut conn, 3);
        seed_candidates(&conn);
        // A selection pointing at a candidate that does not exist — the kind of
        // damage an offline edit with foreign keys off leaves behind. The
        // rebuild's `foreign_key_check` is what refuses to bless it.
        conn.pragma_update(None, "foreign_keys", false).unwrap();
        conn.execute(
            "INSERT INTO example_selections (word_id, slot, ex_cand_id, selected_by)
             VALUES (7, 2, 9999, 'auto')",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();

        let err = ensure_schema(&mut conn).unwrap_err();
        assert!(matches!(err, StoreError::Conflict(_)), "{err}");
        assert!(err.to_string().contains("dangling"), "{err}");
        // Nothing was committed: the database is still at v3 and still narrow.
        let ver: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver, 3);
        assert!(!accepts_source(&conn, "example_candidates", "tatoeba"));
    }

    /// A rung is a no-op when the column is already there, so a contract sync
    /// that lands ahead of an old database cannot brick the ladder.
    #[test]
    fn a_rung_whose_change_is_already_present_is_skipped() {
        let mut conn = Connection::open_in_memory().unwrap();
        // The pathological case: the current contract DDL — which already
        // ships `word_count` *and* the widened source unions — labelled as an
        // older version, so the 2 → 3 and 3 → 4 rungs both run over it.
        conn.execute_batch(WORKING_DB_SQL).unwrap();
        seed_candidates(&conn);
        conn.pragma_update(None, "user_version", 2).unwrap();
        let before = live_ddl(&conn, "example_candidates").unwrap();

        assert_eq!(
            migrate(&mut conn).unwrap(),
            (SCHEMA_USER_VERSION - 2) as usize,
            "the rungs still run"
        );
        assert_eq!(
            columns(&conn, "releases")
                .iter()
                .filter(|c| *c == "word_count")
                .count(),
            1
        );
        assert_eq!(
            live_ddl(&conn, "example_candidates").unwrap(),
            before,
            "a table that already has the contract shape is not rebuilt"
        );
        // The rows were never touched, because the table was never rebuilt.
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM example_candidates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
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
            ("table", "clip_scores"),
            ("table", "tag_category"),
            ("table", "tag"),
            ("table", "candidate_tag"),
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
        for (table, column) in [
            ("words", "core_ready"),
            ("words", "zh_gloss"),
            ("releases", "word_count"),
        ] {
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
