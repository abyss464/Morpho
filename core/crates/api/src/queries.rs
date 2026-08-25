//! Read queries backing the admin API.
//!
//! All of them run on a pooled read-only connection via `Store::read`.

use rusqlite::{Connection, OptionalExtension, ToSql};

use morpho_domain::event::EventRecord;
use morpho_store::error::{Result, StoreError};
use morpho_store::queries as shared;

use crate::dto::*;

fn parse_blockers(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

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
    let rows = stmt.query_map(rusqlite::params![limit], map_event)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
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
    let rows = stmt.query_map(refs.as_slice(), map_event)?;
    Ok(Page {
        items: rows.collect::<rusqlite::Result<Vec<_>>>()?,
        total,
    })
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

pub fn dashboard(conn: &Connection) -> Result<Dashboard> {
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
            definitions: assets.definitions,
            examples: assets.examples,
            images: assets.images,
            tts: DashboardTts {
                ready: assets.tts_ready,
                missing: assets.tts_missing,
                failed: assets.tts_failed,
            },
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

// ---------------------------------------------------------------------------
// Word list
// ---------------------------------------------------------------------------

/// Number of texts this word needs spoken that have no `ready` TTS asset.
/// Three independent correlated scalar subqueries, one per TTS kind.
const TTS_MISSING_SQL: &str = "(
    (SELECT COUNT(*) FROM active_words aw
       LEFT JOIN tts_assets a ON a.kind = 'word' AND a.text = aw.lemma AND a.status = 'ready'
      WHERE aw.word_id = w.word_id AND a.tts_id IS NULL)
  + (SELECT COUNT(*) FROM definition_selections ds
       JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
       LEFT JOIN tts_assets a ON a.kind = 'definition' AND a.text = dc.text AND a.status = 'ready'
      WHERE ds.word_id = w.word_id AND ds.enabled = 1 AND a.tts_id IS NULL)
  + (SELECT COUNT(*) FROM example_selections es
       JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
       LEFT JOIN tts_assets a ON a.kind = 'example' AND a.text = ec.text AND a.status = 'ready'
      WHERE es.word_id = w.word_id AND a.tts_id IS NULL)
)";

pub fn word_list(conn: &Connection, query: &WordListQuery) -> Result<Page<WordRollup>> {
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
                (SELECT COUNT(*) FROM example_selections es WHERE es.word_id = w.word_id),
                {TTS_MISSING_SQL}
         FROM words w
         {where_sql}
         ORDER BY COALESCE(w.frequency_rank, 9223372036854775807), w.word_id
         LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok(WordRollup {
            word_id: row.get(0)?,
            lemma: row.get(1)?,
            role: row.get(2)?,
            ready: row.get::<_, i64>(3)? != 0,
            blockers: parse_blockers(&row.get::<_, String>(4)?),
            has_image: row.get::<_, i64>(5)? != 0,
            sense_count: row.get(6)?,
            example_count: row.get(7)?,
            tts_missing: row.get(8)?,
        })
    })?;
    Ok(Page {
        items: rows.collect::<rusqlite::Result<Vec<_>>>()?,
        total,
    })
}

// ---------------------------------------------------------------------------
// Word detail
// ---------------------------------------------------------------------------

pub fn word_fields(conn: &Connection, word_id: i64) -> Result<WordFields> {
    conn.query_row(
        "SELECT word_id, lemma, role, aux_status, phonetic, frequency_rank, etymology,
                etymology_source, ready, blockers, created_by, created_at
         FROM words WHERE word_id = ?1",
        rusqlite::params![word_id],
        |row| {
            Ok(WordFields {
                word_id: row.get(0)?,
                lemma: row.get(1)?,
                role: row.get(2)?,
                aux_status: row.get(3)?,
                phonetic: row.get(4)?,
                frequency_rank: row.get(5)?,
                etymology: row.get(6)?,
                etymology_source: row.get(7)?,
                ready: row.get::<_, i64>(8)? != 0,
                blockers: parse_blockers(&row.get::<_, String>(9)?),
                created_by: row.get(10)?,
                created_at: row.get(11)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("word {word_id}")))
}

fn definition_slots(conn: &Connection, word_id: i64) -> Result<Vec<DefinitionSlot>> {
    let mut stmt = conn.prepare(
        "SELECT def_cand_id, word_id, pos, text, text_hash, source, source_ref, parent_cand_id,
                status, auto_score, scorer_ver, created_by, created_at
         FROM definition_candidates WHERE word_id = ?1
         ORDER BY pos, def_cand_id",
    )?;
    let candidates: Vec<DefinitionCandidateDto> = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok(DefinitionCandidateDto {
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
                scorer_ver: row.get(10)?,
                created_by: row.get(11)?,
                created_at: row.get(12)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = conn.prepare(
        "SELECT pos, def_cand_id, is_primary, enabled, selected_by, pinned, approved,
                approved_hash, approved_by, approved_at, selection_rev, updated_at
         FROM definition_selections WHERE word_id = ?1",
    )?;
    let selections: Vec<(String, DefinitionSelectionDto)> = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                DefinitionSelectionDto {
                    def_cand_id: row.get(1)?,
                    is_primary: row.get::<_, i64>(2)? != 0,
                    enabled: row.get::<_, i64>(3)? != 0,
                    selected_by: row.get(4)?,
                    pinned: row.get::<_, i64>(5)? != 0,
                    approved: row.get::<_, i64>(6)? != 0,
                    approved_hash: row.get(7)?,
                    approved_by: row.get(8)?,
                    approved_at: row.get(9)?,
                    selection_rev: row.get(10)?,
                    updated_at: row.get(11)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut positions: Vec<String> = candidates.iter().map(|c| c.pos.clone()).collect();
    positions.extend(selections.iter().map(|(pos, _)| pos.clone()));
    positions.sort();
    positions.dedup();

    Ok(positions
        .into_iter()
        .map(|pos| DefinitionSlot {
            selection: selections
                .iter()
                .find(|(slot_pos, _)| slot_pos == &pos)
                .map(|(_, sel)| sel.clone()),
            candidates: candidates
                .iter()
                .filter(|c| c.pos == pos)
                .cloned()
                .collect(),
            pos,
        })
        .collect())
}

fn example_block(conn: &Connection, word_id: i64) -> Result<ExampleBlock> {
    let mut stmt = conn.prepare(
        "SELECT ex_cand_id, text, text_hash, hl_start, hl_end, source, status, auto_score, created_at
         FROM example_candidates WHERE word_id = ?1 ORDER BY ex_cand_id",
    )?;
    let candidates = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok(ExampleCandidateDto {
                ex_cand_id: row.get(0)?,
                text: row.get(1)?,
                text_hash: row.get(2)?,
                hl_start: row.get(3)?,
                hl_end: row.get(4)?,
                source: row.get(5)?,
                status: row.get(6)?,
                auto_score: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = conn.prepare(
        "SELECT slot, ex_cand_id, selected_by, pinned, approved, approved_hash, approved_by,
                approved_at, selection_rev, updated_at
         FROM example_selections WHERE word_id = ?1 ORDER BY slot",
    )?;
    let slots = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok(ExampleSelectionDto {
                slot: row.get(0)?,
                ex_cand_id: row.get(1)?,
                selected_by: row.get(2)?,
                pinned: row.get::<_, i64>(3)? != 0,
                approved: row.get::<_, i64>(4)? != 0,
                approved_hash: row.get(5)?,
                approved_by: row.get(6)?,
                approved_at: row.get(7)?,
                selection_rev: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(ExampleBlock { slots, candidates })
}

fn image_block(conn: &Connection, word_id: i64) -> Result<ImageBlock> {
    let mut stmt = conn.prepare(
        "SELECT img_cand_id, file_hash, pos, width, height, source, source_ref, license,
                status, auto_score, created_at
         FROM image_candidates WHERE word_id = ?1 ORDER BY img_cand_id",
    )?;
    let candidates = stmt
        .query_map(rusqlite::params![word_id], |row| {
            Ok(ImageCandidateDto {
                img_cand_id: row.get(0)?,
                file_hash: row.get(1)?,
                pos: row.get(2)?,
                width: row.get(3)?,
                height: row.get(4)?,
                source: row.get(5)?,
                source_ref: row.get(6)?,
                license: row.get(7)?,
                status: row.get(8)?,
                auto_score: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let selection = conn
        .query_row(
            "SELECT s.img_cand_id, c.file_hash, s.selected_by, s.pinned, s.approved,
                    s.approved_hash, s.approved_by, s.approved_at, s.selection_rev, s.updated_at
             FROM image_selections s
             JOIN image_candidates c ON c.img_cand_id = s.img_cand_id
             WHERE s.word_id = ?1",
            rusqlite::params![word_id],
            |row| {
                Ok(ImageSelectionDto {
                    img_cand_id: row.get(0)?,
                    file_hash: row.get(1)?,
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

    Ok(ImageBlock {
        selection,
        candidates,
    })
}

fn tts_status(conn: &Connection, word_id: i64) -> Result<Vec<TtsStatusDto>> {
    let mut stmt = conn.prepare(
        "SELECT d.kind, d.text, a.status, a.file_hash, a.duration_ms
         FROM (
             SELECT 'word' AS kind, lemma AS text FROM active_words WHERE word_id = ?1
             UNION
             SELECT 'definition', dc.text
               FROM definition_selections ds
               JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
              WHERE ds.word_id = ?1 AND ds.enabled = 1
             UNION
             SELECT 'example', ec.text
               FROM example_selections es
               JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
              WHERE es.word_id = ?1
         ) d
         LEFT JOIN tts_assets a ON a.kind = d.kind AND a.text = d.text
         ORDER BY d.kind, d.text",
    )?;
    let rows = stmt.query_map(rusqlite::params![word_id], |row| {
        let status: Option<String> = row.get(2)?;
        Ok(TtsStatusDto {
            kind: row.get(0)?,
            text: row.get(1)?,
            status: status.unwrap_or_else(|| "missing".to_string()),
            file_hash: row.get(3)?,
            duration_ms: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn distractors(conn: &Connection, word_id: i64) -> Result<Vec<DistractorDto>> {
    let mut stmt = conn.prepare(
        "SELECT d.rank, w.word_id, w.lemma, w.ready, w.blockers
         FROM distractors d JOIN words w ON w.word_id = d.distractor_word_id
         WHERE d.word_id = ?1 ORDER BY d.rank",
    )?;
    let rows = stmt.query_map(rusqlite::params![word_id], |row| {
        let ready = row.get::<_, i64>(3)? != 0;
        let blockers = parse_blockers(&row.get::<_, String>(4)?);
        Ok(DistractorDto {
            rank: row.get(0)?,
            word_id: row.get(1)?,
            lemma: row.get(2)?,
            // `words` has no core_ready column; derive it the way the readiness
            // split does: core is satisfied when the only outstanding blockers
            // are about this word's own distractors.
            core_ready: ready || blockers.iter().all(|b| b.starts_with("distractor_")),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn word_detail(conn: &Connection, word_id: i64) -> Result<WordDetail> {
    let word = word_fields(conn, word_id)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLUMNS} FROM events
         WHERE (entity_type = 'word' AND entity_id = ?1)
            OR (entity_type IN ('definition_candidate','definition_selection',
                                'example_candidate','example_selection',
                                'image_candidate','image_selection')
                AND (entity_id = ?1 OR entity_id LIKE ?1 || ':%'))
         ORDER BY event_id DESC LIMIT 20"
    ))?;
    let recent_events = stmt
        .query_map(rusqlite::params![word_id.to_string()], map_event)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(WordDetail {
        definitions: definition_slots(conn, word_id)?,
        examples: example_block(conn, word_id)?,
        image: image_block(conn, word_id)?,
        tts: tts_status(conn, word_id)?,
        distractors: distractors(conn, word_id)?,
        recent_events,
        word,
    })
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
