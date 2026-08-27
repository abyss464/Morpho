//! Read-side helpers shared by the CLI, the reconciler, the exporter and the
//! admin API.
//!
//! Anything that is naturally a *view* is read from the view — those can never
//! be stale (README Part 3).

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use morpho_domain::job::{JobKey, JobKind, JobStatus, RateKey, SubjectRef, SubjectType};
use morpho_domain::types::{AuxStatus, Role, TtsKind};

use crate::error::Result;

/// Coarse counts used by `morphod status` and the dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WordCounts {
    pub total: i64,
    pub target: i64,
    pub base: i64,
    /// Auxiliaries in `active` status only (admin-api.md wave-2 ruling #8).
    pub auxiliary: i64,
    pub auxiliary_retired: i64,
    pub active: i64,
    pub ready: i64,
    pub core_ready: i64,
    pub blocked: i64,
}

pub fn word_counts(conn: &Connection) -> Result<WordCounts> {
    let mut counts = conn.query_row(
        "SELECT
             COUNT(*),
             COALESCE(SUM(role = 'target'), 0),
             COALESCE(SUM(role = 'base'), 0),
             COALESCE(SUM(role = 'auxiliary' AND aux_status = 'active'), 0),
             COALESCE(SUM(role = 'auxiliary' AND aux_status = 'retired'), 0)
         FROM words",
        [],
        |row| {
            Ok(WordCounts {
                total: row.get(0)?,
                target: row.get(1)?,
                base: row.get(2)?,
                auxiliary: row.get(3)?,
                auxiliary_retired: row.get(4)?,
                ..WordCounts::default()
            })
        },
    )?;
    // Readiness only means anything for words that are actually in scope — and a
    // gloss anchor never becomes ready, so counting it would put a permanent
    // untouchable number in the operator's `blocked` column.
    let (active, ready, core_ready) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(ready = 1), 0), COALESCE(SUM(core_ready = 1), 0)
         FROM active_words WHERE zh_gloss IS NULL",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    counts.active = active;
    counts.ready = ready;
    counts.core_ready = core_ready;
    counts.blocked = (active - ready).max(0);
    Ok(counts)
}

/// `AssetRollup` in admin-ui/src/api/types.ts: how many active words have this
/// asset, how many are still without one, and how many have exhausted every
/// source for it.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct AssetRollup {
    pub ready: i64,
    pub missing: i64,
    pub failed: i64,
}

/// Asset-side counts used by the dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetCounts {
    pub definitions: AssetRollup,
    pub examples: AssetRollup,
    pub images: AssetRollup,
    pub tts: AssetRollup,
    pub definition_candidates: i64,
    pub example_candidates: i64,
    pub image_candidates: i64,
}

/// Per-word asset rollups, keyed by the `job_state` kind that feeds each slot.
///
/// * `ready`   — the slot is filled and approved;
/// * `missing` — the slot is empty or unapproved;
/// * `failed`  — every source for that slot is dead or waived, so nothing more
///   will arrive without a human.
///
/// Gloss anchors are outside all three: they need no asset, so counting them as
/// `missing` would name work that is already done.
fn slot_rollup(conn: &Connection, approved_sql: &str, job_kind: JobKind) -> Result<AssetRollup> {
    let ready: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM active_words w WHERE w.{NOT_ANCHORED} AND {approved_sql}"),
        [],
        |row| row.get(0),
    )?;
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM active_words WHERE {NOT_ANCHORED}"),
        [],
        |row| row.get(0),
    )?;
    // "Failed" means no further source will produce this asset without a
    // human: every fan-out subject for the slot is dead or waived.
    let failed: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT w.word_id)
             FROM active_words w
             JOIN job_state j
               ON j.subject_type = 'word'
              AND j.kind = ?1
              AND (j.subject_id = CAST(w.word_id AS TEXT)
                   OR j.subject_id LIKE CAST(w.word_id AS TEXT) || ':%')
             WHERE w.{NOT_ANCHORED} AND j.status IN ('dead','waived')"
        ),
        rusqlite::params![job_kind.as_str()],
        |row| row.get(0),
    )?;
    Ok(AssetRollup {
        ready,
        missing: (total - ready).max(0),
        failed,
    })
}

pub fn asset_counts(conn: &Connection) -> Result<AssetCounts> {
    let scalar = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |row| row.get(0))?) };
    Ok(AssetCounts {
        definitions: slot_rollup(
            conn,
            "EXISTS (SELECT 1 FROM definition_selections ds
                      WHERE ds.word_id = w.word_id AND ds.enabled = 1 AND ds.is_primary = 1
                        AND ds.approved = 1)",
            JobKind::FetchDefinitions,
        )?,
        examples: slot_rollup(
            conn,
            "EXISTS (SELECT 1 FROM example_selections es
                      WHERE es.word_id = w.word_id AND es.slot = 1 AND es.approved = 1)",
            JobKind::FetchExamples,
        )?,
        images: slot_rollup(
            conn,
            "EXISTS (SELECT 1 FROM image_selections i
                      WHERE i.word_id = w.word_id AND i.approved = 1)",
            JobKind::FetchImages,
        )?,
        // Filled in by the caller, which knows the voice configuration.
        tts: AssetRollup::default(),
        definition_candidates: scalar("SELECT COUNT(*) FROM definition_candidates")?,
        example_candidates: scalar("SELECT COUNT(*) FROM example_candidates")?,
        image_candidates: scalar("SELECT COUNT(*) FROM image_candidates")?,
    })
}

/// The `tts_desired` view with a `word_id` on every row, narrowed to the words
/// the factory is actually building.
///
/// Three callers need to know *whose* clip a desired text is — the readiness
/// pass, the word list and the word detail — and the contract's view cannot say,
/// so the join is written once here. The `zh_gloss IS NULL` filter is what keeps
/// a gloss anchor out of the synthesis queue: an anchor is read in Chinese, not
/// spoken (admin-api.md ruling #18a).
pub const DESIRED_TTS_SQL: &str = "SELECT word_id, kind, text FROM (
         SELECT w.word_id, 'word' AS kind, w.lemma AS text
           FROM active_words w WHERE w.zh_gloss IS NULL
         UNION ALL
         SELECT ds.word_id, 'definition', dc.text
           FROM definition_selections ds
           JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
           JOIN active_words w ON w.word_id = ds.word_id AND w.zh_gloss IS NULL
          WHERE ds.enabled = 1
         UNION ALL
         SELECT es.word_id, 'example', ec.text
           FROM example_selections es
           JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
           JOIN active_words w ON w.word_id = es.word_id AND w.zh_gloss IS NULL
     )";

/// The desired TTS texts, as `(kind, text)` pairs.
///
/// The caller combines each row with the current [`morpho_domain::TtsConfig`]
/// to get an `input_hash`, because SQLite cannot compute blake3.
pub fn tts_desired(conn: &Connection) -> Result<Vec<(TtsKind, String)>> {
    let mut stmt = conn.prepare(&format!("SELECT kind, text FROM ({DESIRED_TTS_SQL})"))?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (kind, text) = row?;
        match kind.parse::<TtsKind>() {
            Ok(kind) => out.push((kind, text)),
            Err(_) => tracing::warn!(kind, "ignoring unknown tts_desired kind"),
        }
    }
    Ok(out)
}

/// State of one `tts_assets` row, looked up by `input_hash`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtsAssetRow {
    pub input_hash: String,
    pub status: String,
    pub file_hash: Option<String>,
    pub duration_ms: Option<i64>,
    pub engine_ver: String,
}

/// Every `tts_assets` row, keyed by `input_hash`.
pub fn tts_assets(conn: &Connection) -> Result<HashMap<String, TtsAssetRow>> {
    let mut stmt = conn
        .prepare("SELECT input_hash, status, file_hash, duration_ms, engine_ver FROM tts_assets")?;
    let rows = stmt.query_map([], |row| {
        Ok(TtsAssetRow {
            input_hash: row.get(0)?,
            status: row.get(1)?,
            file_hash: row.get(2)?,
            duration_ms: row.get(3)?,
            engine_ver: row.get(4)?,
        })
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let row = row?;
        out.insert(row.input_hash.clone(), row);
    }
    Ok(out)
}

/// TTS inputs the engine has stopped working on, by `input_hash`.
///
/// Admin-api.md wave-3 ruling #13: one definition of "given up" for both the
/// per-text `TtsStatusView.status` and the word's `tts_failed` blocker, so the
/// console can never show a `missing` clip on a word it also calls failed.
/// `dead` is the retry budget running out; `waived` is an operator saying the
/// need is satisfied some other way. Neither will produce audio without a
/// human, so both read as `failed`.
pub fn abandoned_tts_inputs(conn: &Connection) -> Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT subject_id FROM job_state
         WHERE kind = 'synth_tts' AND subject_type = 'tts_input'
           AND status IN ('dead','waived')",
    )?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<std::collections::HashSet<_>>>()?;
    Ok(rows)
}

/// Has the engine given up on this TTS input?
///
/// The asset row and the job row are two independent records of the same
/// verdict; either one is enough.
pub fn tts_given_up(
    input_hash: &str,
    assets: &HashMap<String, TtsAssetRow>,
    abandoned: &std::collections::HashSet<String>,
) -> bool {
    abandoned.contains(input_hash)
        || assets
            .get(input_hash)
            .is_some_and(|asset| asset.status == "failed")
}

/// Count of open out-of-scope queue rows.
pub fn oos_open_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM oos_queue WHERE status = 'open'",
        [],
        |row| row.get(0),
    )?)
}

/// Count of dead-lettered jobs.
pub fn dead_letter_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM job_state WHERE status = 'dead'",
        [],
        |row| row.get(0),
    )?)
}

/// One persisted job state row.
#[derive(Debug, Clone)]
pub struct JobStateRow {
    pub key: JobKey,
    pub rate_key: RateKey,
    pub status: JobStatus,
    pub attempts: i64,
    pub next_retry_at: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

/// Load every `job_state` row. The table only holds failures, so it stays
/// small enough to read wholesale each reconcile pass.
pub fn job_states(conn: &Connection) -> Result<Vec<JobStateRow>> {
    let mut stmt = conn.prepare(
        "SELECT kind, subject_type, subject_id, rate_key, status, attempts,
                next_retry_at, last_error, updated_at
         FROM job_state
         ORDER BY kind, subject_type, subject_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (kind, subject_type, subject_id, rate_key, status, attempts, next, err, updated) = row?;
        // Unknown vocabulary means a newer build wrote the row; skip rather
        // than fail the whole pass.
        let (Ok(kind), Ok(subject_type), Ok(rate_key), Ok(status)) = (
            kind.parse::<JobKind>(),
            subject_type.parse::<SubjectType>(),
            rate_key.parse::<RateKey>(),
            status.parse::<JobStatus>(),
        ) else {
            tracing::warn!(kind, subject_type, "skipping unrecognized job_state row");
            continue;
        };
        out.push(JobStateRow {
            key: JobKey::new(kind, SubjectRef::new(subject_type, subject_id)),
            rate_key,
            status,
            attempts,
            next_retry_at: next,
            last_error: err,
            updated_at: updated,
        });
    }
    Ok(out)
}

/// Lane limits as configured in the `rate_limits` table.
#[derive(Debug, Clone, Copy)]
pub struct RateLimitRow {
    pub rate_key: RateKey,
    pub max_concurrency: i64,
    pub refill_per_min: f64,
    pub burst: i64,
}

pub fn rate_limits(conn: &Connection) -> Result<Vec<RateLimitRow>> {
    let mut stmt =
        conn.prepare("SELECT rate_key, max_concurrency, refill_per_min, burst FROM rate_limits")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (key, max_concurrency, refill_per_min, burst) = row?;
        match key.parse::<RateKey>() {
            Ok(rate_key) => out.push(RateLimitRow {
                rate_key,
                max_concurrency,
                refill_per_min,
                burst,
            }),
            Err(_) => tracing::warn!(rate_key = %key, "ignoring unknown rate_limits row"),
        }
    }
    Ok(out)
}

/// Resolve a lemma to its word id (case-insensitive, per the schema collation).
pub fn word_id_by_lemma(conn: &Connection, lemma: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT word_id FROM words WHERE lemma = ?1",
            rusqlite::params![lemma],
            |row| row.get(0),
        )
        .optional()?)
}

/// A word row as every rule and the exporter want it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordRow {
    pub word_id: i64,
    pub lemma: String,
    pub role: Role,
    pub aux_status: Option<AuxStatus>,
    pub phonetic: Option<String>,
    pub frequency_rank: Option<i64>,
    pub etymology: Option<String>,
    pub etymology_source: Option<String>,
    /// A non-empty Chinese gloss makes this word an anchor, not a student-facing
    /// word (admin-api.md ruling #18a).
    pub zh_gloss: Option<String>,
    pub zh_gloss_source: Option<String>,
    pub ready: bool,
    pub core_ready: bool,
    pub blockers: Vec<String>,
}

impl WordRow {
    pub fn is_active(&self) -> bool {
        match self.role {
            Role::Target => true,
            Role::Auxiliary => self.aux_status == Some(AuxStatus::Active),
            Role::Base => false,
        }
    }
}

const WORD_COLUMNS: &str = "word_id, lemma, role, aux_status, phonetic, frequency_rank, \
                            etymology, etymology_source, zh_gloss, zh_gloss_source, \
                            ready, core_ready, blockers";

/// The `WHERE` clause that separates student-facing words from gloss anchors.
///
/// A glossed word is a terminator of the readability chain, not a word anybody
/// learns: it takes no assets, no plan slot and no readiness verdict. Clearing
/// the gloss puts it straight back into normal life, which is why this is a
/// predicate over live state rather than a status column (admin-api.md ruling
/// #18a).
pub const NOT_ANCHORED: &str = "zh_gloss IS NULL";

fn map_word(row: &rusqlite::Row<'_>) -> rusqlite::Result<WordRow> {
    let role: String = row.get(2)?;
    let aux: Option<String> = row.get(3)?;
    Ok(WordRow {
        word_id: row.get(0)?,
        lemma: row.get(1)?,
        role: role.parse().unwrap_or(Role::Target),
        aux_status: aux.and_then(|raw| raw.parse().ok()),
        phonetic: row.get(4)?,
        frequency_rank: row.get(5)?,
        etymology: row.get(6)?,
        etymology_source: row.get(7)?,
        zh_gloss: row.get(8)?,
        zh_gloss_source: row.get(9)?,
        ready: row.get::<_, i64>(10)? != 0,
        core_ready: row.get::<_, i64>(11)? != 0,
        blockers: morpho_domain::blocker::parse_blockers(&row.get::<_, String>(12)?),
    })
}

/// Every word, ordered deterministically by `(frequency_rank, word_id)`.
pub fn all_words(conn: &Connection) -> Result<Vec<WordRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {WORD_COLUMNS} FROM words
         ORDER BY COALESCE(frequency_rank, 9223372036854775807), word_id"
    ))?;
    let rows = stmt
        .query_map([], map_word)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Every word in `active_words` that is not a gloss anchor, in the same
/// deterministic order.
///
/// This is the factory's word set: derivation, selection, the plan, the
/// distractor pool, readiness and the release lexicon all fan out from here, so
/// excluding anchors in one place is what keeps them out of all six.
pub fn active_words(conn: &Connection) -> Result<Vec<WordRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {WORD_COLUMNS} FROM active_words WHERE {NOT_ANCHORED}
         ORDER BY COALESCE(frequency_rank, 9223372036854775807), word_id"
    ))?;
    let rows = stmt
        .query_map([], map_word)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn word_by_id(conn: &Connection, word_id: i64) -> Result<Option<WordRow>> {
    Ok(conn
        .query_row(
            &format!("SELECT {WORD_COLUMNS} FROM words WHERE word_id = ?1"),
            rusqlite::params![word_id],
            map_word,
        )
        .optional()?)
}

/// `def_dependencies`: edges from a word to the target/auxiliary words its
/// selected definitions rely on. Deterministically ordered.
///
/// An edge into a gloss anchor is dropped, and that single omission is what
/// ruling #18a buys: readiness sees the dependency as satisfied, the plan does
/// not have to place the anchor, and the export closure no longer drags the
/// dependent off the boat. Edges *out* of an anchor go too — an anchor's own
/// definition is nobody's reading obligation.
pub fn dependency_edges(conn: &Connection) -> Result<Vec<(i64, i64)>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT DISTINCT d.word_id, d.depends_on_word_id
         FROM def_dependencies d
         JOIN active_words a ON a.word_id = d.word_id AND a.{NOT_ANCHORED}
         JOIN active_words b ON b.word_id = d.depends_on_word_id AND b.{NOT_ANCHORED}
         ORDER BY d.word_id, d.depends_on_word_id"
    ))?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// One gloss anchor: a word that terminates the readability chain in Chinese
/// instead of being learned (admin-api.md ruling #18a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossAnchor {
    pub word_id: i64,
    /// The lemma as it appears in definition tokens.
    pub lemma: String,
    pub zh_gloss: String,
}

/// Every glossed word, in ascending id.
pub fn gloss_anchors(conn: &Connection) -> Result<Vec<GlossAnchor>> {
    let mut stmt = conn.prepare(
        "SELECT word_id, lemma, zh_gloss FROM words
         WHERE zh_gloss IS NOT NULL ORDER BY word_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(GlossAnchor {
                word_id: row.get(0)?,
                lemma: row.get(1)?,
                zh_gloss: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// `(word_id, anchor_word_id)` — which words' enabled selected definitions
/// actually mention a gloss anchor.
///
/// The edges [`dependency_edges`] deliberately drops, kept so the exporter can
/// ship exactly the anchors a shipped word needs and no orphans. The role
/// filter of `def_dependencies` is not applied: a glossed base word is just as
/// tappable in the app as a glossed auxiliary.
pub fn gloss_anchor_refs(conn: &Connection) -> Result<Vec<(i64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT ds.word_id, w.word_id
         FROM definition_selections ds
         JOIN def_tokens t ON t.def_cand_id = ds.def_cand_id
         JOIN words w ON w.lemma = t.lemma
         WHERE ds.enabled = 1 AND w.zh_gloss IS NOT NULL AND w.word_id <> ds.word_id
         ORDER BY ds.word_id, w.word_id",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// `(word_id, lemma)` — tokens of enabled selected definitions that resolve to
/// no `words` row at all. The `oos_occurrences` view, flattened.
pub fn unresolved_definition_tokens(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT word_id, oos_lemma FROM oos_occurrences ORDER BY word_id, oos_lemma",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Distractor edges, deterministically ordered.
pub fn distractor_edges(conn: &Connection) -> Result<Vec<(i64, i64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT word_id, rank, distractor_word_id FROM distractors ORDER BY word_id, rank",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// One bound distractor with both lemmas resolved.
///
/// [`distractor_edges`] is enough for the reconciler, which only ever asks
/// "which ranks are taken"; auditing a binding needs the words themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistractorPair {
    pub word_id: i64,
    /// The lemma of the word being quizzed.
    pub lemma: String,
    pub rank: i64,
    pub distractor_word_id: i64,
    pub distractor_lemma: String,
}

/// Every distractor row joined to both lemmas, ordered by `(word_id, rank)`.
///
/// No role or readiness filter: a binding that violates a selection rule is
/// worth reporting wherever it sits, including on a word that has since left
/// the active pool.
pub fn distractor_pairs(conn: &Connection) -> Result<Vec<DistractorPair>> {
    let mut stmt = conn.prepare(
        "SELECT d.word_id, w.lemma, d.rank, d.distractor_word_id, x.lemma
         FROM distractors d
         JOIN words w ON w.word_id = d.word_id
         JOIN words x ON x.word_id = d.distractor_word_id
         ORDER BY d.word_id, d.rank",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(DistractorPair {
                word_id: row.get(0)?,
                lemma: row.get(1)?,
                rank: row.get(2)?,
                distractor_word_id: row.get(3)?,
                distractor_lemma: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Primary POS for each word, from its `is_primary = 1` definition selection.
///
/// Returns `(word_id, pos)` for every word that has a primary selection enabled.
pub fn primary_pos_map(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare(
        "SELECT word_id, pos FROM definition_selections
         WHERE is_primary = 1 AND enabled = 1
         ORDER BY word_id",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// All enabled definition-selection POS values for each word.
///
/// Returns `(word_id, pos)` pairs — a word with noun and verb senses appears
/// twice. Used by distractor selection to detect POS overlap.
pub fn word_pos_set(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare(
        "SELECT word_id, pos FROM definition_selections
         WHERE enabled = 1
         ORDER BY word_id, pos",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Lemmas currently visible in `oos_occurrences`, folded and deduplicated.
pub fn oos_lemmas(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT DISTINCT oos_lemma FROM oos_occurrences ORDER BY oos_lemma")?;
    let rows = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Out-of-scope lemmas whose queue row is not yet resolved, per word.
///
/// A lemma with no queue row at all counts as pending too: the sync rule has
/// simply not run yet, and pretending otherwise would flip a word ready for one
/// cycle.
pub fn pending_oos_words(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT o.word_id
         FROM oos_occurrences o
         LEFT JOIN oos_queue q ON q.oos_lemma = o.oos_lemma
         WHERE q.status IS NULL OR q.status = 'open'
         ORDER BY o.word_id",
    )?;
    let rows = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// `source_fetch` completion markers, keyed by `(kind, word_id, source)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFetchRow {
    pub fetched_at: String,
    pub result_count: i64,
}

pub fn source_fetches(
    conn: &Connection,
    kind: &str,
) -> Result<BTreeMap<(i64, String), SourceFetchRow>> {
    let mut stmt = conn.prepare(
        "SELECT word_id, source, fetched_at, result_count FROM source_fetch WHERE kind = ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![kind], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            SourceFetchRow {
                fetched_at: row.get(2)?,
                result_count: row.get(3)?,
            },
        ))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (word_id, source, value) = row?;
        out.insert((word_id, source), value);
    }
    Ok(out)
}

/// Current plan summary, if a plan has been built.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSummary {
    pub plan_id: i64,
    pub input_hash: String,
    pub algo_ver: String,
    pub params_json: String,
    pub stats_json: Option<String>,
    pub built_at: String,
    pub group_count: i64,
    pub word_count: i64,
}

pub fn current_plan(conn: &Connection) -> Result<Option<PlanSummary>> {
    let plan = conn
        .query_row(
            "SELECT plan_id, input_hash, algo_ver, params_json, stats_json, built_at
             FROM plan_artifacts WHERE is_current = 1",
            [],
            |row| {
                Ok(PlanSummary {
                    plan_id: row.get(0)?,
                    input_hash: row.get(1)?,
                    algo_ver: row.get(2)?,
                    params_json: row.get(3)?,
                    stats_json: row.get(4)?,
                    built_at: row.get(5)?,
                    group_count: 0,
                    word_count: 0,
                })
            },
        )
        .optional()?;
    let Some(mut plan) = plan else {
        return Ok(None);
    };
    plan.group_count = conn.query_row(
        "SELECT COUNT(*) FROM plan_groups WHERE plan_id = ?1",
        rusqlite::params![plan.plan_id],
        |row| row.get(0),
    )?;
    plan.word_count = conn.query_row(
        "SELECT COUNT(*) FROM plan_words WHERE plan_id = ?1",
        rusqlite::params![plan.plan_id],
        |row| row.get(0),
    )?;
    Ok(Some(plan))
}

/// One word's placement in a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanPlacement {
    pub learning_order: i64,
    pub group_seq: i64,
}

/// Placements of the current plan, keyed by word id.
pub fn plan_placements(conn: &Connection, plan_id: i64) -> Result<HashMap<i64, PlanPlacement>> {
    let mut stmt = conn
        .prepare("SELECT word_id, learning_order, group_seq FROM plan_words WHERE plan_id = ?1")?;
    let rows = stmt.query_map(rusqlite::params![plan_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            PlanPlacement {
                learning_order: row.get(1)?,
                group_seq: row.get(2)?,
            },
        ))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (word_id, placement) = row?;
        out.insert(word_id, placement);
    }
    Ok(out)
}

/// Group types of a plan, keyed by `group_seq`.
pub fn plan_group_types(conn: &Connection, plan_id: i64) -> Result<BTreeMap<i64, String>> {
    let mut stmt = conn.prepare(
        "SELECT group_seq, group_type FROM plan_groups WHERE plan_id = ?1 ORDER BY group_seq",
    )?;
    let rows = stmt.query_map(rusqlite::params![plan_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (seq, kind) = row?;
        out.insert(seq, kind);
    }
    Ok(out)
}

/// Every media file currently referenced by something that keeps it alive:
/// candidates, TTS assets and release manifests (README Part 3 §"媒体 GC").
pub fn referenced_media(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT file_hash FROM (
             SELECT file_hash FROM image_candidates
             UNION SELECT file_hash FROM tts_assets WHERE file_hash IS NOT NULL
             UNION SELECT file_hash FROM release_manifests
         ) ORDER BY file_hash",
    )?;
    let rows = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Every registered media file with its current GC stamp.
pub fn media_registry(conn: &Connection) -> Result<Vec<(String, Option<String>)>> {
    let mut stmt =
        conn.prepare("SELECT file_hash, gc_eligible_at FROM media_files ORDER BY file_hash")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
