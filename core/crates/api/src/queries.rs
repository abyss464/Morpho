//! Read queries backing the admin API.
//!
//! All of them run on a pooled read-only connection via `Store::read`.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, ToSql};

use morpho_domain::blocker::parse_blockers;
use morpho_domain::event::EventRecord;
use morpho_domain::tts::TtsConfig;
use morpho_domain::types::TtsKind;
use morpho_store::error::{Result, StoreError};
use morpho_store::queries as shared;

use crate::dto::*;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

fn map_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<EventRecord> {
    let detail: Option<String> = row.get(6)?;
    Ok(EventRecord {
        event_id: row.get(0)?,
        ts: row.get(1)?,
        actor: row.get(2)?,
        entity_type: row.get(3)?,
        entity_id: row.get(4)?,
        action: row.get(5)?,
        detail: detail
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or(serde_json::Value::Null),
    })
}

const EVENT_COLUMNS: &str = "event_id, ts, actor, entity_type, entity_id, action, detail";

pub fn recent_events(conn: &Connection, limit: i64) -> Result<Vec<EventRecord>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLUMNS} FROM events ORDER BY event_id DESC LIMIT ?1"
    ))?;
    let rows = stmt
        .query_map(rusqlite::params![limit], map_event)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn events_page(conn: &Connection, query: &EventQuery) -> Result<Page<EventRecord>> {
    let mut clauses: Vec<&str> = Vec::new();
    let mut params: Vec<Box<dyn ToSql>> = Vec::new();
    if let Some(entity_type) = &query.entity_type {
        clauses.push("entity_type = ?");
        params.push(Box::new(entity_type.clone()));
    }
    if let Some(entity_id) = &query.entity_id {
        clauses.push("entity_id = ?");
        params.push(Box::new(entity_id.clone()));
    }
    if let Some(action) = &query.action {
        clauses.push("action = ?");
        params.push(Box::new(action.clone()));
    }
    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };

    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM events {where_sql}"),
        refs.as_slice(),
        |row| row.get(0),
    )?;

    let mut paged: Vec<Box<dyn ToSql>> = params;
    let page = query.pagination();
    paged.push(Box::new(page.limit()));
    paged.push(Box::new(page.offset()));
    let refs: Vec<&dyn ToSql> = paged.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLUMNS} FROM events {where_sql}
         ORDER BY event_id DESC LIMIT ? OFFSET ?"
    ))?;
    let items = stmt
        .query_map(refs.as_slice(), map_event)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Page { items, total })
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

pub fn dashboard(conn: &Connection, tts: &TtsConfig) -> Result<Dashboard> {
    let words = shared::word_counts(conn)?;
    let assets = shared::asset_counts(conn)?;
    let plan = shared::current_plan(conn)?;
    Ok(Dashboard {
        words: DashboardWords {
            total: words.total,
            target: words.target,
            auxiliary: words.auxiliary,
            ready: words.ready,
            blocked: words.blocked,
        },
        assets: DashboardAssets {
            definitions: rollup(assets.definitions),
            examples: rollup(assets.examples),
            images: rollup(assets.images),
            tts: tts_rollup(conn, tts)?,
        },
        oos_open: shared::oos_open_count(conn)?,
        dead_letters: shared::dead_letter_count(conn)?,
        plan: plan.map(|p| DashboardPlan {
            plan_id: p.plan_id,
            built_at: p.built_at,
            group_count: p.group_count,
        }),
        recent_events: recent_events(conn, 20)?,
    })
}

fn rollup(source: shared::AssetRollup) -> AssetRollup {
    AssetRollup {
        ready: source.ready,
        missing: source.missing,
        failed: source.failed,
    }
}

/// TTS coverage of the whole desired set against the configured voice.
///
/// Buckets by the same rule as `TtsStatusView.status` (ruling #13), so the
/// dashboard headline and the per-word detail cannot disagree.
fn tts_rollup(conn: &Connection, config: &TtsConfig) -> Result<AssetRollup> {
    let assets = shared::tts_assets(conn)?;
    let abandoned = shared::abandoned_tts_inputs(conn)?;
    let mut out = AssetRollup::default();
    let mut seen = std::collections::HashSet::new();
    for (kind, text) in shared::tts_desired(conn)? {
        let hash = config.input_hash(kind, &text);
        if !seen.insert(hash.clone()) {
            continue;
        }
        if assets.get(&hash).is_some_and(|a| a.status == "ready") {
            out.ready += 1;
        } else if shared::tts_given_up(&hash, &assets, &abandoned) {
            out.failed += 1;
        } else {
            out.missing += 1;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Word list
// ---------------------------------------------------------------------------

/// Texts this word needs spoken. Combined with the voice config in Rust,
/// because SQLite cannot compute the content address.
fn desired_texts_by_word(conn: &Connection) -> Result<HashMap<i64, Vec<(TtsKind, String)>>> {
    let mut stmt = conn.prepare(morpho_store::queries::DESIRED_TTS_SQL)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out: HashMap<i64, Vec<(TtsKind, String)>> = HashMap::new();
    for (word_id, kind, text) in rows {
        if let Ok(kind) = kind.parse::<TtsKind>() {
            out.entry(word_id).or_default().push((kind, text));
        }
    }
    Ok(out)
}

pub fn word_list(
    conn: &Connection,
    query: &WordListQuery,
    tts: &TtsConfig,
) -> Result<Page<WordListItem>> {
    let mut clauses: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn ToSql>> = Vec::new();

    if let Some(role) = &query.role {
        clauses.push("w.role = ?".into());
        params.push(Box::new(role.clone()));
    }
    if let Some(ready) = query.ready {
        clauses.push("w.ready = ?".into());
        params.push(Box::new(i64::from(ready)));
    }
    if let Some(blocker) = &query.blocker {
        clauses.push("EXISTS (SELECT 1 FROM json_each(w.blockers) je WHERE je.value = ?)".into());
        params.push(Box::new(blocker.clone()));
    }
    if let Some(group) = query.group {
        clauses.push(
            "EXISTS (SELECT 1 FROM plan_words pw
                     JOIN plan_artifacts pa ON pa.plan_id = pw.plan_id AND pa.is_current = 1
                     WHERE pw.word_id = w.word_id AND pw.group_seq = ?)"
                .into(),
        );
        params.push(Box::new(group));
    }
    if let Some(q) = &query.q {
        clauses.push("w.lemma LIKE '%' || ? || '%'".into());
        params.push(Box::new(q.clone()));
    }
    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };

    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM words w {where_sql}"),
        refs.as_slice(),
        |row| row.get(0),
    )?;

    let mut paged: Vec<Box<dyn ToSql>> = params;
    let page = query.pagination();
    paged.push(Box::new(page.limit()));
    paged.push(Box::new(page.offset()));
    let refs: Vec<&dyn ToSql> = paged.iter().map(|p| p.as_ref()).collect();

    let sql = format!(
        "SELECT w.word_id, w.lemma, w.role, w.ready, w.blockers,
                EXISTS (SELECT 1 FROM image_selections i WHERE i.word_id = w.word_id),
                (SELECT COUNT(*) FROM definition_selections ds
                  WHERE ds.word_id = w.word_id AND ds.enabled = 1),
                (SELECT COUNT(*) FROM example_selections es WHERE es.word_id = w.word_id)
         FROM words w
         {where_sql}
         ORDER BY COALESCE(w.frequency_rank, 9223372036854775807), w.word_id
         LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(refs.as_slice(), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? != 0,
                parse_blockers(&row.get::<_, String>(4)?),
                row.get::<_, i64>(5)? != 0,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let desired = desired_texts_by_word(conn)?;
    let assets = shared::tts_assets(conn)?;
    let items = rows
        .into_iter()
        .map(
            |(word_id, lemma, role, ready, blockers, has_image, sense_count, example_count)| {
                let tts_missing = desired
                    .get(&word_id)
                    .map(|texts| {
                        texts
                            .iter()
                            .filter(|(kind, text)| {
                                let hash = tts.input_hash(*kind, text);
                                !assets
                                    .get(&hash)
                                    .is_some_and(|asset| asset.status == "ready")
                            })
                            .count() as i64
                    })
                    .unwrap_or(0);
                WordListItem {
                    word_id,
                    lemma,
                    role,
                    ready,
                    blockers,
                    has_image,
                    sense_count,
                    example_count,
                    tts_missing,
                }
            },
        )
        .collect();

    Ok(Page { items, total })
}

// ---------------------------------------------------------------------------
// Word detail
// ---------------------------------------------------------------------------

pub fn word_fields(conn: &Connection, word_id: i64) -> Result<Word> {
    conn.query_row(
        "SELECT word_id, lemma, role, aux_status, phonetic, frequency_rank, etymology,
                etymology_source, zh_gloss, zh_gloss_source, ready, blockers,
                created_by, created_at
         FROM words WHERE word_id = ?1",
        rusqlite::params![word_id],
        |row| {
            Ok(Word {
                word_id: row.get(0)?,
                lemma: row.get(1)?,
                role: row.get(2)?,
                aux_status: row.get(3)?,
                phonetic: row.get(4)?,
                frequency_rank: row.get(5)?,
                etymology: row.get(6)?,
                etymology_source: row.get(7)?,
                zh_gloss: row.get(8)?,
                zh_gloss_source: row.get(9)?,
                ready: row.get::<_, i64>(10)? != 0,
                blockers: parse_blockers(&row.get::<_, String>(11)?),
                created_by: row.get(12)?,
                created_at: row.get(13)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("word {word_id}")))
}

fn definition_candidates(conn: &Connection, word_id: i64) -> Result<Vec<DefinitionCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT def_cand_id, word_id, pos, text, text_hash, source, source_ref, parent_cand_id,
                status, auto_score, score_detail, scorer_ver, created_by, created_at
         FROM definition_candidates WHERE word_id = ?1
         ORDER BY pos, def_cand_id",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok(DefinitionCandidate {
                def_cand_id: row.get(0)?,
                word_id: row.get(1)?,
                pos: row.get(2)?,
                text: row.get(3)?,
                text_hash: row.get(4)?,
                source: row.get(5)?,
                source_ref: row.get(6)?,
                parent_cand_id: row.get(7)?,
                status: row.get(8)?,
                auto_score: row.get(9)?,
                score_detail: decode_json(row.get(10)?),
                scorer_ver: row.get(11)?,
                created_by: row.get(12)?,
                created_at: row.get(13)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn definition_selections(conn: &Connection, word_id: i64) -> Result<Vec<DefinitionSelection>> {
    let mut stmt = conn.prepare(
        "SELECT word_id, pos, def_cand_id, is_primary, enabled, selected_by, pinned, approved,
                approved_hash, approved_by, approved_at, selection_rev, updated_at
         FROM definition_selections WHERE word_id = ?1",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok(DefinitionSelection {
                word_id: row.get(0)?,
                pos: row.get(1)?,
                def_cand_id: row.get(2)?,
                is_primary: row.get::<_, i64>(3)? != 0,
                enabled: row.get::<_, i64>(4)? != 0,
                selected_by: row.get(5)?,
                pinned: row.get::<_, i64>(6)? != 0,
                approved: row.get::<_, i64>(7)? != 0,
                approved_hash: row.get(8)?,
                approved_by: row.get(9)?,
                approved_at: row.get(10)?,
                selection_rev: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn definition_slots(conn: &Connection, word_id: i64) -> Result<Vec<DefinitionSlotView>> {
    let candidates = definition_candidates(conn, word_id)?;
    let selections = definition_selections(conn, word_id)?;

    let mut positions: Vec<String> = candidates.iter().map(|c| c.pos.clone()).collect();
    positions.extend(selections.iter().map(|s| s.pos.clone()));
    positions.sort();
    positions.dedup();

    let mut slots: Vec<DefinitionSlotView> = positions
        .into_iter()
        .map(|pos| DefinitionSlotView {
            selection: selections.iter().find(|s| s.pos == pos).cloned(),
            candidates: candidates
                .iter()
                .filter(|c| c.pos == pos)
                .cloned()
                .collect(),
            pos,
        })
        .collect();
    // The contract says "grouped by pos, primary sense first".
    slots.sort_by_key(|slot| {
        (
            !slot
                .selection
                .as_ref()
                .is_some_and(|selection| selection.is_primary),
            slot.pos.clone(),
        )
    });
    Ok(slots)
}

fn example_candidates(conn: &Connection, word_id: i64) -> Result<Vec<ExampleCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT ex_cand_id, word_id, text, text_hash, hl_start, hl_end, source, source_ref,
                status, auto_score, score_detail, scorer_ver, created_by, created_at
         FROM example_candidates WHERE word_id = ?1 ORDER BY ex_cand_id",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok(ExampleCandidate {
                ex_cand_id: row.get(0)?,
                word_id: row.get(1)?,
                text: row.get(2)?,
                text_hash: row.get(3)?,
                hl_start: row.get(4)?,
                hl_end: row.get(5)?,
                source: row.get(6)?,
                source_ref: row.get(7)?,
                status: row.get(8)?,
                auto_score: row.get(9)?,
                score_detail: decode_json(row.get(10)?),
                scorer_ver: row.get(11)?,
                created_by: row.get(12)?,
                created_at: row.get(13)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn example_slots(conn: &Connection, word_id: i64) -> Result<Vec<ExampleSlotView>> {
    let candidates = example_candidates(conn, word_id)?;
    let mut stmt = conn.prepare(
        "SELECT word_id, slot, ex_cand_id, selected_by, pinned, approved, approved_hash,
                approved_by, approved_at, selection_rev, updated_at
         FROM example_selections WHERE word_id = ?1 ORDER BY slot",
    )?;
    let selections = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok(ExampleSelection {
                word_id: row.get(0)?,
                slot: row.get(1)?,
                ex_cand_id: row.get(2)?,
                selected_by: row.get(3)?,
                pinned: row.get::<_, i64>(4)? != 0,
                approved: row.get::<_, i64>(5)? != 0,
                approved_hash: row.get(6)?,
                approved_by: row.get(7)?,
                approved_at: row.get(8)?,
                selection_rev: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Always three entries, slots 1..3, even when empty: the console renders a
    // fixed set of slot cards.
    Ok((1..=3)
        .map(|slot| ExampleSlotView {
            slot,
            selection: selections.iter().find(|s| s.slot == slot).cloned(),
            candidates: candidates.clone(),
        })
        .collect())
}

fn image_slot(conn: &Connection, word_id: i64) -> Result<ImageSlotView> {
    let mut stmt = conn.prepare(
        "SELECT img_cand_id, word_id, pos, file_hash, width, height, source, source_ref, license,
                query_used, status, auto_score, score_detail, scorer_ver, created_by, created_at
         FROM image_candidates WHERE word_id = ?1 ORDER BY img_cand_id",
    )?;
    let candidates = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok(ImageCandidate {
                img_cand_id: row.get(0)?,
                word_id: row.get(1)?,
                pos: row.get(2)?,
                file_hash: row.get(3)?,
                width: row.get(4)?,
                height: row.get(5)?,
                source: row.get(6)?,
                source_ref: row.get(7)?,
                license: row.get(8)?,
                query_used: row.get(9)?,
                status: row.get(10)?,
                auto_score: row.get(11)?,
                score_detail: decode_json(row.get(12)?),
                scorer_ver: row.get(13)?,
                created_by: row.get(14)?,
                created_at: row.get(15)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let selection = conn
        .query_row(
            "SELECT word_id, img_cand_id, selected_by, pinned, approved, approved_hash,
                    approved_by, approved_at, selection_rev, updated_at
             FROM image_selections WHERE word_id = ?1",
            rusqlite::params![word_id],
            |row| {
                Ok(ImageSelection {
                    word_id: row.get(0)?,
                    img_cand_id: row.get(1)?,
                    selected_by: row.get(2)?,
                    pinned: row.get::<_, i64>(3)? != 0,
                    approved: row.get::<_, i64>(4)? != 0,
                    approved_hash: row.get(5)?,
                    approved_by: row.get(6)?,
                    approved_at: row.get(7)?,
                    selection_rev: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )
        .optional()?;

    Ok(ImageSlotView {
        selection,
        candidates,
    })
}

fn tts_status(conn: &Connection, word_id: i64, config: &TtsConfig) -> Result<Vec<TtsStatusView>> {
    let mut stmt = conn.prepare(
        // A gloss anchor is never spoken, so it lists no clips at all rather
        // than a wall of `missing` the operator can do nothing about.
        "SELECT kind, text, pos, slot FROM (
             SELECT 'word' AS kind, lemma AS text, NULL AS pos, NULL AS slot
               FROM active_words WHERE word_id = ?1 AND zh_gloss IS NULL
             UNION ALL
             SELECT 'definition', dc.text, ds.pos, NULL
               FROM definition_selections ds
               JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
               JOIN active_words w ON w.word_id = ds.word_id AND w.zh_gloss IS NULL
              WHERE ds.word_id = ?1 AND ds.enabled = 1
             UNION ALL
             SELECT 'example', ec.text, NULL, es.slot
               FROM example_selections es
               JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
               JOIN active_words w ON w.word_id = es.word_id AND w.zh_gloss IS NULL
              WHERE es.word_id = ?1
         ) ORDER BY kind, text",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let assets = shared::tts_assets(conn)?;
    let abandoned = shared::abandoned_tts_inputs(conn)?;
    let errors = tts_errors(conn)?;

    let mut out = Vec::with_capacity(rows.len());
    for (kind_raw, text, pos, slot) in rows {
        let Ok(kind) = kind_raw.parse::<TtsKind>() else {
            continue;
        };
        let input_hash = config.input_hash(kind, &text);
        let asset = assets.get(&input_hash);
        let status = if asset.is_some_and(|a| a.status == "ready") {
            "ready"
        } else if shared::tts_given_up(&input_hash, &assets, &abandoned) {
            // Wave-3 ruling #13: a dead or waived `synth_tts` row is `failed`,
            // with or without an asset row, so this agrees with the word's
            // `tts_failed` blocker.
            "failed"
        } else {
            // Wave-2 ruling #6: desired but not yet synthesized — never
            // attempted, or still inside its retry budget.
            "missing"
        };
        out.push(TtsStatusView {
            kind: kind_raw,
            text_hash: morpho_domain::hash::text_hash(&text),
            voice: config.voice.clone(),
            engine: config.engine.clone(),
            engine_ver: asset
                .map(|a| a.engine_ver.clone())
                .unwrap_or_else(|| config.engine_ver.clone()),
            status: status.to_string(),
            file_hash: asset.and_then(|a| a.file_hash.clone()),
            duration_ms: asset.and_then(|a| a.duration_ms),
            last_error: errors.get(&input_hash).cloned(),
            r#ref: match (pos, slot) {
                (None, None) => None,
                (pos, slot) => Some(TtsRef { pos, slot }),
            },
            input_hash,
            text,
        });
    }
    Ok(out)
}

/// Last error of each TTS synthesis that has a `job_state` row.
fn tts_errors(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT subject_id, last_error FROM job_state
         WHERE kind = 'synth_tts' AND subject_type = 'tts_input' AND last_error IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn distractors(conn: &Connection, word_id: i64) -> Result<Vec<DistractorView>> {
    let mut stmt = conn.prepare(
        "SELECT d.rank, w.word_id, w.lemma, w.core_ready, w.blockers, d.bound_at, d.bound_by
         FROM distractors d JOIN words w ON w.word_id = d.distractor_word_id
         WHERE d.word_id = ?1 ORDER BY d.rank",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok(DistractorView {
                rank: row.get(0)?,
                word_id: row.get(1)?,
                lemma: row.get(2)?,
                // Wave-2 ruling #5: read the reconciler-owned column.
                core_ready: row.get::<_, i64>(3)? != 0,
                blockers: parse_blockers(&row.get::<_, String>(4)?),
                bound_at: row.get(5)?,
                bound_by: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn word_detail(conn: &Connection, word_id: i64, tts: &TtsConfig) -> Result<WordDetail> {
    let word = word_fields(conn, word_id)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLUMNS} FROM events
         WHERE (entity_type = 'word' AND entity_id = ?1)
            OR (entity_type IN ('definition_candidate','definition_selection',
                                'example_candidate','example_selection',
                                'image_candidate','image_selection','distractor')
                AND (entity_id = ?1 OR entity_id LIKE ?1 || ':%'))
         ORDER BY event_id DESC LIMIT 20"
    ))?;
    let recent_events = stmt
        .query_map(rusqlite::params![word_id.to_string()], map_event)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(WordDetail {
        definitions: definition_slots(conn, word_id)?,
        examples: example_slots(conn, word_id)?,
        image: image_slot(conn, word_id)?,
        tts: tts_status(conn, word_id, tts)?,
        distractors: distractors(conn, word_id)?,
        recent_events,
        word,
    })
}

// ---------------------------------------------------------------------------
// OOV queue
// ---------------------------------------------------------------------------

pub fn oov_page(conn: &Connection, query: &OovQuery) -> Result<Page<OovQueueEntry>> {
    let page = query.pagination();
    let (where_sql, status): (&str, Option<String>) = match &query.status {
        Some(status) => ("WHERE status = ?1", Some(status.clone())),
        None => ("", None),
    };

    let total: i64 = match &status {
        Some(status) => conn.query_row(
            &format!("SELECT COUNT(*) FROM oos_queue {where_sql}"),
            rusqlite::params![status],
            |row| row.get(0),
        )?,
        None => conn.query_row("SELECT COUNT(*) FROM oos_queue", [], |row| row.get(0))?,
    };

    let sql = format!(
        "SELECT oos_lemma, status, first_seen, resolved_by, resolved_at, notes
         FROM oos_queue {where_sql}
         ORDER BY first_seen DESC, oos_lemma LIMIT ?2 OFFSET ?3"
    );
    let mut stmt = conn.prepare(&sql)?;
    let map = |row: &rusqlite::Row<'_>| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    };
    let rows = match &status {
        Some(status) => stmt
            .query_map(rusqlite::params![status, page.limit(), page.offset()], map)?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        None => stmt
            .query_map(
                rusqlite::params![Option::<String>::None, page.limit(), page.offset()],
                map,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    };

    let mut items = Vec::with_capacity(rows.len());
    for (oos_lemma, status, first_seen, resolved_by, resolved_at, notes) in rows {
        let occurrences = oov_occurrences(conn, &oos_lemma)?;
        items.push(OovQueueEntry {
            occurrence_count: occurrences.len(),
            occurrences,
            oos_lemma,
            status,
            first_seen,
            resolved_by,
            resolved_at,
            notes,
        });
    }
    Ok(Page { items, total })
}

fn oov_occurrences(conn: &Connection, oos_lemma: &str) -> Result<Vec<OovOccurrence>> {
    let mut stmt = conn.prepare(
        "SELECT o.word_id, w.lemma, ds.pos, o.def_cand_id, dc.text, o.hits,
                (SELECT r.text FROM definition_candidates r
                  WHERE r.parent_cand_id = o.def_cand_id
                    AND r.source = 'llm_rewrite' AND r.status = 'available'
                  ORDER BY r.def_cand_id DESC LIMIT 1)
         FROM oos_occurrences o
         JOIN words w ON w.word_id = o.word_id
         JOIN definition_candidates dc ON dc.def_cand_id = o.def_cand_id
         JOIN definition_selections ds ON ds.def_cand_id = o.def_cand_id
        WHERE o.oos_lemma = ?1
        ORDER BY o.word_id, o.def_cand_id",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![oos_lemma], |row| {
            Ok(OovOccurrence {
                word_id: row.get(0)?,
                lemma: row.get(1)?,
                pos: row.get(2)?,
                def_cand_id: row.get(3)?,
                text: row.get(4)?,
                hits: row.get(5)?,
                suggested_rewrite: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Dead letters
// ---------------------------------------------------------------------------

/// `GET /api/dead-letters`, optionally narrowed to one dispatcher lane.
///
/// Wave-3 ruling #14: `rate_key` is a filter, not a validated enum. A blank
/// value means "no filter"; a value that names no lane simply matches nothing,
/// which is a 200 with an empty page rather than an error. Widening the result
/// on an unrecognized key would make a typo look like a broken filter.
pub fn dead_letters(
    conn: &Connection,
    page: Pagination,
    rate_key: Option<&str>,
) -> Result<Page<DeadLetter>> {
    let rate_key = rate_key.map(str::trim).filter(|value| !value.is_empty());
    let filter = match rate_key {
        Some(_) => "AND rate_key = ?1",
        None => "AND ?1 IS NULL",
    };

    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM job_state WHERE status = 'dead' {filter}"),
        rusqlite::params![rate_key],
        |row| row.get(0),
    )?;
    let mut stmt = conn.prepare(&format!(
        "SELECT kind, subject_type, subject_id, rate_key, status, attempts, next_retry_at,
                last_error, updated_at
         FROM job_state WHERE status = 'dead' {filter}
         ORDER BY updated_at DESC, kind, subject_id
         LIMIT ?2 OFFSET ?3"
    ))?;
    let rows = stmt
        .query_map(
            rusqlite::params![rate_key, page.limit(), page.offset()],
            |row| {
                Ok(DeadLetter {
                    kind: row.get(0)?,
                    subject_type: row.get(1)?,
                    subject_id: row.get(2)?,
                    rate_key: row.get(3)?,
                    status: row.get(4)?,
                    attempts: row.get(5)?,
                    next_retry_at: row.get(6)?,
                    last_error: row.get(7)?,
                    updated_at: row.get(8)?,
                    subject: DeadLetterSubject {
                        word_id: None,
                        lemma: None,
                        label: String::new(),
                    },
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let items = rows
        .into_iter()
        .map(|mut letter| {
            letter.subject = resolve_subject(conn, &letter.subject_type, &letter.subject_id);
            letter
        })
        .collect();
    Ok(Page { items, total })
}

/// Human-facing context for one job subject.
///
/// Word subjects may carry a `:source` suffix from the per-source fan-out, so
/// the numeric prefix is what gets resolved.
pub fn resolve_subject(
    conn: &Connection,
    subject_type: &str,
    subject_id: &str,
) -> DeadLetterSubject {
    match subject_type {
        "word" => {
            let (head, source) = match subject_id.split_once(':') {
                Some((head, source)) => (head, Some(source)),
                None => (subject_id, None),
            };
            let word_id: Option<i64> = head.parse().ok();
            let lemma = word_id.and_then(|id| {
                conn.query_row(
                    "SELECT lemma FROM words WHERE word_id = ?1",
                    rusqlite::params![id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .ok()
                .flatten()
            });
            let label = match (&lemma, source) {
                (Some(lemma), Some(source)) => format!("{lemma} ({source})"),
                (Some(lemma), None) => lemma.clone(),
                (None, _) => format!("word {subject_id}"),
            };
            DeadLetterSubject {
                word_id,
                lemma,
                label,
            }
        }
        "def_candidate" => {
            let cand_id: Option<i64> = subject_id.parse().ok();
            let found = cand_id.and_then(|id| {
                conn.query_row(
                    "SELECT dc.word_id, w.lemma, dc.text FROM definition_candidates dc
                     JOIN words w ON w.word_id = dc.word_id WHERE dc.def_cand_id = ?1",
                    rusqlite::params![id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .ok()
                .flatten()
            });
            match found {
                Some((word_id, lemma, text)) => DeadLetterSubject {
                    word_id: Some(word_id),
                    label: format!("{lemma}: {}", excerpt(&text)),
                    lemma: Some(lemma),
                },
                None => DeadLetterSubject {
                    word_id: None,
                    lemma: None,
                    label: format!("definition candidate {subject_id}"),
                },
            }
        }
        "tts_input" => {
            let text: Option<String> = conn
                .query_row(
                    "SELECT text FROM tts_assets WHERE input_hash = ?1",
                    rusqlite::params![subject_id],
                    |row| row.get(0),
                )
                .optional()
                .ok()
                .flatten();
            DeadLetterSubject {
                word_id: None,
                lemma: None,
                label: match text {
                    Some(text) => format!("tts: {}", excerpt(&text)),
                    None => format!("tts {}", &subject_id[..12.min(subject_id.len())]),
                },
            }
        }
        _ => DeadLetterSubject {
            word_id: None,
            lemma: None,
            label: subject_id.to_string(),
        },
    }
}

fn excerpt(text: &str) -> String {
    const LIMIT: usize = 60;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let truncated: String = text.chars().take(LIMIT).collect();
    format!("{truncated}…")
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

pub fn plan_summary(conn: &Connection) -> Result<Option<PlanSummary>> {
    let Some(plan) = shared::current_plan(conn)? else {
        return Ok(None);
    };

    let mut stmt = conn.prepare(
        "SELECT g.group_seq, g.group_type,
                COUNT(pw.word_id),
                COALESCE(SUM(w.ready = 1), 0),
                COALESCE(MIN(CASE WHEN pw.learning_order =
                    (SELECT MIN(p2.learning_order) FROM plan_words p2
                      WHERE p2.plan_id = g.plan_id AND p2.group_seq = g.group_seq)
                    THEN w.lemma END), ''),
                COALESCE(MIN(CASE WHEN pw.learning_order =
                    (SELECT MAX(p2.learning_order) FROM plan_words p2
                      WHERE p2.plan_id = g.plan_id AND p2.group_seq = g.group_seq)
                    THEN w.lemma END), '')
         FROM plan_groups g
         LEFT JOIN plan_words pw ON pw.plan_id = g.plan_id AND pw.group_seq = g.group_seq
         LEFT JOIN words w ON w.word_id = pw.word_id
         WHERE g.plan_id = ?1
         GROUP BY g.group_seq, g.group_type
         ORDER BY g.group_seq",
    )?;
    let groups = stmt
        .query_map(rusqlite::params![plan.plan_id], |row| {
            Ok(PlanGroupSummary {
                group_seq: row.get(0)?,
                group_type: row.get(1)?,
                word_count: row.get(2)?,
                ready_count: row.get(3)?,
                first_lemma: row.get(4)?,
                last_lemma: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let stats: PlanStats = plan
        .stats_json
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default();
    let params: serde_json::Value = serde_json::from_str(&plan.params_json)
        .unwrap_or(serde_json::Value::Object(Default::default()));

    Ok(Some(PlanSummary {
        diff: plan_diff(conn, plan.plan_id)?,
        plan_id: plan.plan_id,
        input_hash: plan.input_hash,
        algo_ver: plan.algo_ver,
        params,
        is_current: true,
        built_at: plan.built_at,
        stats,
        groups,
    }))
}

/// Counts against the previous artifact, so the console can show what a
/// rebuild actually did.
fn plan_diff(conn: &Connection, plan_id: i64) -> Result<PlanDiff> {
    let previous: Option<i64> = conn
        .query_row(
            "SELECT plan_id FROM plan_artifacts WHERE plan_id < ?1 ORDER BY plan_id DESC LIMIT 1",
            rusqlite::params![plan_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(previous_plan_id) = previous else {
        let added: i64 = conn.query_row(
            "SELECT COUNT(*) FROM plan_words WHERE plan_id = ?1",
            rusqlite::params![plan_id],
            |row| row.get(0),
        )?;
        return Ok(PlanDiff {
            previous_plan_id: None,
            added,
            removed: 0,
            reordered: 0,
        });
    };

    let scalar = |sql: &str| -> Result<i64> {
        Ok(
            conn.query_row(sql, rusqlite::params![plan_id, previous_plan_id], |row| {
                row.get(0)
            })?,
        )
    };
    Ok(PlanDiff {
        previous_plan_id: Some(previous_plan_id),
        added: scalar(
            "SELECT COUNT(*) FROM plan_words a
             WHERE a.plan_id = ?1
               AND NOT EXISTS (SELECT 1 FROM plan_words b
                                WHERE b.plan_id = ?2 AND b.word_id = a.word_id)",
        )?,
        removed: scalar(
            "SELECT COUNT(*) FROM plan_words b
             WHERE b.plan_id = ?2
               AND NOT EXISTS (SELECT 1 FROM plan_words a
                                WHERE a.plan_id = ?1 AND a.word_id = b.word_id)",
        )?,
        reordered: scalar(
            "SELECT COUNT(*) FROM plan_words a
             JOIN plan_words b ON b.plan_id = ?2 AND b.word_id = a.word_id
             WHERE a.plan_id = ?1 AND a.learning_order <> b.learning_order",
        )?,
    })
}

pub fn plan_group(conn: &Connection, group_seq: i64) -> Result<Option<PlanGroupDetail>> {
    let Some(plan) = shared::current_plan(conn)? else {
        return Ok(None);
    };
    let group_type: Option<String> = conn
        .query_row(
            "SELECT group_type FROM plan_groups WHERE plan_id = ?1 AND group_seq = ?2",
            rusqlite::params![plan.plan_id, group_seq],
            |row| row.get(0),
        )
        .optional()?;
    let Some(group_type) = group_type else {
        return Ok(None);
    };

    let mut stmt = conn.prepare(
        "SELECT w.word_id, w.lemma, w.role, pw.learning_order, pw.group_seq, w.ready, w.blockers
         FROM plan_words pw JOIN words w ON w.word_id = pw.word_id
         WHERE pw.plan_id = ?1 AND pw.group_seq = ?2
         ORDER BY pw.learning_order",
    )?;
    let words = stmt
        .query_map(rusqlite::params![plan.plan_id, group_seq], |row| {
            Ok(PlanWordView {
                word_id: row.get(0)?,
                lemma: row.get(1)?,
                role: row.get(2)?,
                learning_order: row.get(3)?,
                group_seq: row.get(4)?,
                ready: row.get::<_, i64>(5)? != 0,
                blockers: parse_blockers(&row.get::<_, String>(6)?),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(Some(PlanGroupDetail {
        plan_id: plan.plan_id,
        group_seq,
        group_type,
        words,
    }))
}

// ---------------------------------------------------------------------------
// Releases
// ---------------------------------------------------------------------------

pub fn releases(conn: &Connection, page: Pagination) -> Result<Page<Release>> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0))?;
    // Ruling #15: `word_count` is a column of `releases`, not something to be
    // recovered from the audit log.
    let mut stmt = conn.prepare(
        "SELECT r.release_id, r.version, r.plan_id, r.input_hash, r.db_file_hash, r.exported_at,
                r.exported_by, r.notes, r.word_count,
                (SELECT COUNT(*) FROM release_manifests m WHERE m.release_id = r.release_id),
                (SELECT COALESCE(SUM(f.bytes), 0) FROM release_manifests m
                  JOIN media_files f ON f.file_hash = m.file_hash
                 WHERE m.release_id = r.release_id)
         FROM releases r
         ORDER BY r.release_id DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let items = stmt
        .query_map(rusqlite::params![page.limit(), page.offset()], |row| {
            Ok(Release {
                release_id: row.get(0)?,
                version: row.get(1)?,
                plan_id: row.get(2)?,
                input_hash: row.get(3)?,
                db_file_hash: row.get(4)?,
                exported_at: row.get(5)?,
                exported_by: row.get(6)?,
                notes: row.get(7)?,
                word_count: row.get(8)?,
                media_count: row.get(9)?,
                total_bytes: row.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(Page { items, total })
}

// ---------------------------------------------------------------------------
// Media
// ---------------------------------------------------------------------------

pub struct MediaRow {
    pub kind: String,
    pub rel_path: String,
    pub bytes: i64,
}

pub fn media_file(conn: &Connection, file_hash: &str) -> Result<MediaRow> {
    conn.query_row(
        "SELECT kind, rel_path, bytes FROM media_files WHERE file_hash = ?1",
        rusqlite::params![file_hash],
        |row| {
            Ok(MediaRow {
                kind: row.get(0)?,
                rel_path: row.get(1)?,
                bytes: row.get(2)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("media file {file_hash}")))
}

/// Labels for the in-flight half of `GET /jobs`.
pub fn job_labels(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    let mut stmt = conn.prepare("SELECT word_id, lemma FROM words")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (word_id, lemma) in rows {
        out.insert(word_id.to_string(), lemma.clone());
        for source in [
            "freedict",
            "wordnet",
            "wiktionary",
            "morfessor",
            "exam_corpus",
            "unsplash",
            "pexels",
            "pixabay",
            "sdxl",
        ] {
            out.insert(format!("{word_id}:{source}"), format!("{lemma} ({source})"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpts_are_bounded_on_characters_not_bytes() {
        assert_eq!(excerpt("short"), "short");
        let long = "é".repeat(200);
        let cut = excerpt(&long);
        assert!(cut.ends_with('…'));
        assert_eq!(cut.chars().count(), 61);
    }
}
