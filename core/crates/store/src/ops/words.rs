//! Word-list import and single-word creation.

use rusqlite::OptionalExtension;

use morpho_domain::canon::canonicalize;
use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, EventDraft};
use morpho_domain::types::{AuxStatus, CreatedBy, EtymologySource, Role, WordImport};

use super::{OpCtx, WriteResult};
use crate::error::{Result, StoreError};

/// Bulk import of a word list.
#[derive(Debug, Clone)]
pub struct ImportWords {
    pub role: Role,
    pub created_by: CreatedBy,
    pub words: Vec<WordImport>,
}

/// Outcome counters of an import run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImportStats {
    /// New `words` rows.
    pub inserted: usize,
    /// Existing rows whose phonetic / frequency_rank was filled in or corrected.
    pub updated: usize,
    /// Existing rows that already matched.
    pub unchanged: usize,
    /// Blank lemmas and within-file duplicates.
    pub skipped: usize,
}

/// Create exactly one word row.
#[derive(Debug, Clone)]
pub struct CreateWord {
    pub lemma: String,
    pub role: Role,
    pub phonetic: Option<String>,
    pub frequency_rank: Option<i64>,
    pub created_by: CreatedBy,
    /// When the lemma already exists: return it instead of erroring.
    pub if_absent: bool,
}

impl CreateWord {
    pub fn new(lemma: impl Into<String>, role: Role, created_by: CreatedBy) -> Self {
        Self {
            lemma: lemma.into(),
            role,
            phonetic: None,
            frequency_rank: None,
            created_by,
            if_absent: false,
        }
    }
}

fn aux_status_for(role: Role) -> Option<&'static str> {
    // The schema enforces `(role = 'auxiliary') = (aux_status IS NOT NULL)`.
    match role {
        Role::Auxiliary => Some(AuxStatus::Active.as_str()),
        _ => None,
    }
}

struct ExistingWord {
    word_id: i64,
    phonetic: Option<String>,
    frequency_rank: Option<i64>,
}

fn lookup(ctx: &OpCtx<'_, '_>, lemma: &str) -> Result<Option<ExistingWord>> {
    let found = ctx
        .tx
        .query_row(
            "SELECT word_id, phonetic, frequency_rank FROM words WHERE lemma = ?1",
            rusqlite::params![lemma],
            |row| {
                Ok(ExistingWord {
                    word_id: row.get(0)?,
                    phonetic: row.get(1)?,
                    frequency_rank: row.get(2)?,
                })
            },
        )
        .optional()?;
    Ok(found)
}

pub(super) fn import_words(req: ImportWords, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let mut stats = ImportStats::default();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for entry in &req.words {
        let lemma = canonicalize(&entry.word);
        if lemma.is_empty() {
            stats.skipped += 1;
            continue;
        }
        if !seen.insert(lemma.to_lowercase()) {
            stats.skipped += 1;
            continue;
        }

        let phonetic = entry
            .phonetic
            .as_deref()
            .map(canonicalize)
            .filter(|s| !s.is_empty());

        match lookup(ctx, &lemma)? {
            None => {
                ctx.tx.execute(
                    "INSERT INTO words (lemma, role, aux_status, phonetic, frequency_rank, created_by, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        lemma,
                        req.role.as_str(),
                        aux_status_for(req.role),
                        phonetic,
                        entry.frequency_rank,
                        req.created_by.as_str(),
                        ctx.now,
                    ],
                )?;
                let word_id = ctx.tx.last_insert_rowid();
                stats.inserted += 1;
                ctx.touch(EntityType::Word, word_id.to_string());
            }
            Some(existing) => {
                // Role is never rewritten by an import: promotion / retirement
                // is a reconciler or operator decision, not a list decision.
                let new_phonetic = phonetic.or(existing.phonetic.clone());
                let new_rank = entry.frequency_rank.or(existing.frequency_rank);
                if new_phonetic != existing.phonetic || new_rank != existing.frequency_rank {
                    ctx.tx.execute(
                        "UPDATE words SET phonetic = ?2, frequency_rank = ?3 WHERE word_id = ?1",
                        rusqlite::params![existing.word_id, new_phonetic, new_rank],
                    )?;
                    stats.updated += 1;
                    ctx.touch(EntityType::Word, existing.word_id.to_string());
                } else {
                    stats.unchanged += 1;
                }
            }
        }
    }

    if stats.inserted > 0 || stats.updated > 0 {
        ctx.event(
            EventDraft::new(EntityType::Word, "import", Action::WordsImported).detail(
                serde_json::json!({
                    "role": req.role.as_str(),
                    "inserted": stats.inserted,
                    "updated": stats.updated,
                    "unchanged": stats.unchanged,
                    "skipped": stats.skipped,
                }),
            ),
        )?;
    }

    Ok(WriteResult::Import(stats))
}

/// Write a word's etymology and its provenance.
#[derive(Debug, Clone)]
pub struct SetEtymology {
    pub word_id: i64,
    /// `None` records "this source had nothing" without clobbering a better
    /// answer that is already there.
    pub etymology: Option<String>,
    pub source: EtymologySource,
}

pub(super) fn set_etymology(req: SetEtymology, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let text = req
        .etymology
        .as_deref()
        .map(canonicalize)
        .filter(|s| !s.is_empty());

    let existing: Option<(Option<String>, Option<String>)> = ctx
        .tx
        .query_row(
            "SELECT etymology, etymology_source FROM words WHERE word_id = ?1",
            rusqlite::params![req.word_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((current, current_source)) = existing else {
        return Err(StoreError::not_found(format!("word {}", req.word_id)));
    };

    let Some(text) = text else {
        return Ok(WriteResult::Unit);
    };
    // Provenance ranking: a hand-written etymology outranks Wiktionary, which
    // outranks a Morfessor segmentation. A weaker source never overwrites a
    // stronger one that is already recorded.
    let incoming_rank = source_rank(req.source);
    let current_rank = current_source
        .as_deref()
        .and_then(|raw| raw.parse::<EtymologySource>().ok())
        .map(source_rank)
        .unwrap_or(0);
    if current.is_some() && current_rank > incoming_rank {
        return Ok(WriteResult::Unit);
    }
    if current.as_deref() == Some(text.as_str()) && current_rank == incoming_rank {
        return Ok(WriteResult::Unit);
    }

    ctx.tx.execute(
        "UPDATE words SET etymology = ?2, etymology_source = ?3 WHERE word_id = ?1",
        rusqlite::params![req.word_id, text, req.source.as_str()],
    )?;
    ctx.event(
        EventDraft::new(
            EntityType::Word,
            req.word_id.to_string(),
            Action::EtymologySet,
        )
        .detail(serde_json::json!({
            "word_id": req.word_id,
            "source": req.source.as_str(),
            "previous_source": current_source,
        })),
    )?;
    ctx.touch(EntityType::Word, req.word_id.to_string());
    Ok(WriteResult::Unit)
}

const fn source_rank(source: EtymologySource) -> u8 {
    match source {
        EtymologySource::Morfessor => 1,
        EtymologySource::Wiktionary => 2,
        EtymologySource::Manual => 3,
    }
}

/// Flip an auxiliary word between `active` and `retired`.
///
/// Retirement is fully reversible and destroys nothing: the word simply leaves
/// `active_words`, and with it the plan, the TTS desired set and the release
/// (README Part 3 §"辅助词生命周期").
#[derive(Debug, Clone)]
pub struct SetAuxStatus {
    pub word_id: i64,
    pub status: AuxStatus,
    /// Free-text explanation stored in the audit detail.
    pub reason: &'static str,
}

pub(super) fn set_aux_status(req: SetAuxStatus, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let row: Option<(String, Option<String>, String)> = ctx
        .tx
        .query_row(
            "SELECT role, aux_status, lemma FROM words WHERE word_id = ?1",
            rusqlite::params![req.word_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((role, current, lemma)) = row else {
        return Err(StoreError::not_found(format!("word {}", req.word_id)));
    };
    if role != Role::Auxiliary.as_str() {
        return Err(StoreError::conflict(format!(
            "word {} is {role}, not auxiliary",
            req.word_id
        )));
    }
    if current.as_deref() == Some(req.status.as_str()) {
        return Ok(WriteResult::Unit);
    }

    ctx.tx.execute(
        "UPDATE words SET aux_status = ?2 WHERE word_id = ?1",
        rusqlite::params![req.word_id, req.status.as_str()],
    )?;
    let action = match req.status {
        AuxStatus::Active => Action::AuxPromoted,
        AuxStatus::Retired => Action::AuxRetired,
    };
    ctx.event(
        EventDraft::new(EntityType::Word, req.word_id.to_string(), action).detail(
            serde_json::json!({
                "word_id": req.word_id,
                "lemma": lemma,
                "from": current,
                "to": req.status.as_str(),
                "reason": req.reason,
            }),
        ),
    )?;
    ctx.touch(EntityType::Word, req.word_id.to_string());
    Ok(WriteResult::Unit)
}

pub(super) fn create_word(req: CreateWord, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let lemma = canonicalize(&req.lemma);
    if lemma.is_empty() {
        return Err(StoreError::invalid("lemma must not be empty"));
    }

    if let Some(existing) = lookup(ctx, &lemma)? {
        if req.if_absent {
            return Ok(WriteResult::Word {
                word_id: existing.word_id,
                created: false,
            });
        }
        return Err(StoreError::conflict(format!(
            "word already exists: {lemma}"
        )));
    }

    ctx.tx.execute(
        "INSERT INTO words (lemma, role, aux_status, phonetic, frequency_rank, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            lemma,
            req.role.as_str(),
            aux_status_for(req.role),
            req.phonetic.as_deref().map(canonicalize),
            req.frequency_rank,
            req.created_by.as_str(),
            ctx.now,
        ],
    )?;
    let word_id = ctx.tx.last_insert_rowid();

    let action = if req.created_by == CreatedBy::Promotion {
        Action::AuxPromoted
    } else {
        Action::WordCreated
    };
    ctx.event(
        EventDraft::new(EntityType::Word, word_id.to_string(), action).detail(serde_json::json!({
            "lemma": lemma,
            "role": req.role.as_str(),
            "created_by": req.created_by.as_str(),
        })),
    )?;
    ctx.touch(EntityType::Word, word_id.to_string());

    Ok(WriteResult::Word {
        word_id,
        created: true,
    })
}
