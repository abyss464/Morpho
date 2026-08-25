//! Word-list import and single-word creation.

use rusqlite::OptionalExtension;

use morpho_domain::canon::canonicalize;
use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, EventDraft};
use morpho_domain::types::{AuxStatus, CreatedBy, Role, WordImport};

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
