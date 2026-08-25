//! Readiness recomputation.
//!
//! README Part 3: "就绪度是纯 DB 数学，不是任务" — the engine recomputes it
//! inline every pass. This stage gathers the facts in one read, folds them
//! through [`crate::readiness`], and writes back only the rows whose verdict
//! actually moved.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use morpho_domain::event::Actor;
use morpho_domain::job::{JobKind, JobStatus};
use morpho_domain::tts::TtsConfig;
use morpho_domain::types::TtsKind;
use morpho_store::error::Result;
use morpho_store::ops::{ApplyReadiness, ReadinessRow};
use morpho_store::{queries, Store, WriteOp, WriteResult};

use crate::engine::EngineContext;
use crate::readiness::{evaluate_all, WordFacts};

/// Recompute `ready`, `core_ready` and `blockers` for every active word.
pub async fn recompute_readiness(store: &Store, context: &EngineContext) -> Result<usize> {
    let tts = context.tts.clone();
    let rows = store.read(move |conn| collect(conn, &tts)).await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let outcome = store
        .write(
            Actor::Reconciler,
            WriteOp::ApplyReadiness(ApplyReadiness { rows }),
        )
        .await?;
    Ok(match outcome.result {
        WriteResult::Readiness { changed } => changed,
        _ => 0,
    })
}

fn collect(conn: &Connection, tts: &TtsConfig) -> Result<Vec<ReadinessRow>> {
    let active = queries::active_words(conn)?;
    if active.is_empty() {
        return Ok(Vec::new());
    }

    let sense_state = sense_state(conn)?;
    let example_state = example_state(conn)?;
    let image_state = image_state(conn)?;
    let oos_pending: HashSet<i64> = queries::pending_oos_words(conn)?.into_iter().collect();
    let distractors = distractors_by_word(conn)?;
    let plan = queries::current_plan(conn)?;
    let placements = match &plan {
        Some(plan) => queries::plan_placements(conn, plan.plan_id)?,
        None => HashMap::new(),
    };
    let uncovered = uncovered_dependencies(conn, &placements)?;
    let tts_state = tts_state(conn, tts)?;

    let facts: Vec<WordFacts> = active
        .iter()
        .map(|word| {
            let senses = sense_state.get(&word.word_id).copied().unwrap_or_default();
            let examples = example_state
                .get(&word.word_id)
                .copied()
                .unwrap_or_default();
            let image = image_state.get(&word.word_id).copied();
            let tts = tts_state.get(&word.word_id).copied().unwrap_or_default();
            WordFacts {
                word_id: word.word_id,
                has_enabled_sense: senses.enabled > 0,
                has_primary: senses.has_primary,
                primary_approved: senses.primary_approved,
                unapproved_senses: senses.unapproved,
                oos_pending: oos_pending.contains(&word.word_id),
                uncovered_dependency: uncovered.contains(&word.word_id),
                has_example_slot1: examples.slot1,
                unapproved_examples: examples.unapproved,
                has_image: image.is_some(),
                image_approved: image.unwrap_or(false),
                tts_missing: tts.missing,
                tts_failed: tts.failed,
                in_plan: placements.contains_key(&word.word_id),
                distractors: distractors.get(&word.word_id).cloned().unwrap_or_default(),
            }
        })
        .collect();

    Ok(evaluate_all(&facts)
        .into_iter()
        .map(|readiness| ReadinessRow {
            word_id: readiness.word_id,
            ready: readiness.ready,
            core_ready: readiness.core_ready,
            blockers_json: readiness.blockers.to_json(),
        })
        .collect())
}

#[derive(Debug, Clone, Copy, Default)]
struct SenseState {
    enabled: usize,
    has_primary: bool,
    primary_approved: bool,
    unapproved: usize,
}

fn sense_state(conn: &Connection) -> Result<HashMap<i64, SenseState>> {
    let mut stmt = conn.prepare(
        "SELECT word_id,
                SUM(enabled = 1),
                MAX(is_primary = 1),
                MAX(is_primary = 1 AND approved = 1),
                SUM(enabled = 1 AND approved = 0)
         FROM definition_selections
         GROUP BY word_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                SenseState {
                    enabled: row.get::<_, i64>(1)?.max(0) as usize,
                    has_primary: row.get::<_, i64>(2)? != 0,
                    primary_approved: row.get::<_, i64>(3)? != 0,
                    unapproved: row.get::<_, i64>(4)?.max(0) as usize,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

#[derive(Debug, Clone, Copy, Default)]
struct ExampleState {
    slot1: bool,
    unapproved: usize,
}

fn example_state(conn: &Connection) -> Result<HashMap<i64, ExampleState>> {
    let mut stmt = conn.prepare(
        "SELECT word_id, MAX(slot = 1), SUM(approved = 0)
         FROM example_selections GROUP BY word_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                ExampleState {
                    slot1: row.get::<_, i64>(1)? != 0,
                    unapproved: row.get::<_, i64>(2)?.max(0) as usize,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn image_state(conn: &Connection) -> Result<HashMap<i64, bool>> {
    let mut stmt = conn.prepare("SELECT word_id, approved FROM image_selections")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)? != 0))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn distractors_by_word(conn: &Connection) -> Result<HashMap<i64, Vec<(i64, i64)>>> {
    let mut out: HashMap<i64, Vec<(i64, i64)>> = HashMap::new();
    for (word_id, rank, distractor_word_id) in queries::distractor_edges(conn)? {
        out.entry(word_id)
            .or_default()
            .push((rank, distractor_word_id));
    }
    Ok(out)
}

/// Words whose selected definitions depend on something the plan does not
/// place before them.
///
/// The topological order makes this vacuous in a healthy database — which is
/// the point: it is the invariant check that catches a plan that stopped being
/// a plan. Same-group dependencies (an SCC pack) are covered by construction.
fn uncovered_dependencies(
    conn: &Connection,
    placements: &HashMap<i64, queries::PlanPlacement>,
) -> Result<HashSet<i64>> {
    let mut uncovered = HashSet::new();
    if placements.is_empty() {
        // Nothing is placed yet; `not_in_plan` already says so, and reporting
        // every word as dependency-broken on top would be noise.
        return Ok(uncovered);
    }
    for (word_id, depends_on) in queries::dependency_edges(conn)? {
        let (Some(word), Some(dependency)) =
            (placements.get(&word_id), placements.get(&depends_on))
        else {
            uncovered.insert(word_id);
            continue;
        };
        let ok = dependency.group_seq == word.group_seq
            || dependency.learning_order < word.learning_order;
        if !ok {
            uncovered.insert(word_id);
        }
    }
    Ok(uncovered)
}

#[derive(Debug, Clone, Copy, Default)]
struct TtsState {
    missing: usize,
    failed: usize,
}

/// Per-word TTS coverage against the current voice configuration.
///
/// A desired text with no asset is `missing` while its job is still alive, and
/// `failed` once the job is dead or waived — a distinction the console needs
/// and `tts_assets` alone cannot make.
fn tts_state(conn: &Connection, config: &TtsConfig) -> Result<HashMap<i64, TtsState>> {
    let assets = queries::tts_assets(conn)?;
    let dead: HashSet<String> = queries::job_states(conn)?
        .into_iter()
        .filter(|row| matches!(row.status, JobStatus::Dead | JobStatus::Waived))
        .filter(|row| row.key.kind == JobKind::SynthTts)
        .map(|row| row.key.subject.subject_id)
        .collect();

    let mut stmt = conn.prepare(
        "SELECT word_id, kind, text FROM (
             SELECT word_id, 'word' AS kind, lemma AS text FROM active_words
             UNION ALL
             SELECT ds.word_id, 'definition', dc.text
               FROM definition_selections ds
               JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
               JOIN active_words w ON w.word_id = ds.word_id
              WHERE ds.enabled = 1
             UNION ALL
             SELECT es.word_id, 'example', ec.text
               FROM example_selections es
               JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
               JOIN active_words w ON w.word_id = es.word_id
         )",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut out: HashMap<i64, TtsState> = HashMap::new();
    for (word_id, kind, text) in rows {
        let Ok(kind) = kind.parse::<TtsKind>() else {
            continue;
        };
        let input_hash = config.input_hash(kind, &text);
        let ready = assets
            .get(&input_hash)
            .is_some_and(|asset| asset.status == "ready");
        if ready {
            continue;
        }
        let entry = out.entry(word_id).or_default();
        let given_up = dead.contains(&input_hash)
            || assets
                .get(&input_hash)
                .is_some_and(|asset| asset.status == "failed");
        if given_up {
            entry.failed += 1;
        } else {
            entry.missing += 1;
        }
    }
    Ok(out)
}
