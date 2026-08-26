//! Out-of-scope (OOV) queue resolution.
//!
//! Both resolutions are pure state writes; the engine derives every
//! consequence (README Part 3 §"派生 · 分词与依赖", Part 4 example C).

use rusqlite::OptionalExtension;

use morpho_domain::canon::fold_lemma;
use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, EventDraft};
use morpho_domain::types::{
    AuxStatus, CreatedBy, DefinitionSource, GlossSource, OosStatus, Role, SelectedBy,
};

use super::selections::{MintDefinitionCandidate, SetSelection};
use super::words::{CreateWord, SetGloss};
use super::{selections, words, OpCtx, WriteResult};
use crate::error::{Result, StoreError};

/// How an operator disposed of one out-of-scope lemma.
#[derive(Debug, Clone)]
pub enum OovResolution {
    /// Promote the lemma to an active auxiliary word.
    Promote {
        phonetic: Option<String>,
        frequency_rank: Option<i64>,
    },
    /// Mint a rewritten definition that avoids the lemma, and select it.
    Rewrite {
        def_cand_id: i64,
        text: String,
        source: DefinitionSource,
    },
    /// Ground the lemma with a short Chinese gloss instead of teaching it
    /// (admin-api.md ruling #18a). The word row is created if it does not
    /// exist, as an auxiliary — the anchor needs an id to be referenced by, and
    /// clearing the gloss later hands a normal auxiliary back to the factory.
    Gloss {
        zh_gloss: String,
        source: GlossSource,
    },
}

/// Reconcile `oos_queue` against the `oos_occurrences` view.
///
/// Sync rule (README Part 3 §"派生 · 分词与依赖"): a lemma the view reports but
/// the queue does not have is inserted `open`; a queue row that is still `open`
/// but has left the view is `auto_closed` (its definition was rewritten, or the
/// lemma got promoted). Resolved rows are never reopened — coming back into the
/// view after a rewrite was undone inserts nothing, because the row exists.
#[derive(Debug, Clone)]
pub struct SyncOosQueue {
    /// Every lemma currently visible in `oos_occurrences`, folded.
    pub present: Vec<String>,
}

pub(super) fn sync_oos_queue(req: SyncOosQueue, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let present: std::collections::BTreeSet<String> =
        req.present.iter().map(|l| fold_lemma(l)).collect();

    let known: Vec<(String, String)> = {
        let mut stmt = ctx.tx.prepare("SELECT oos_lemma, status FROM oos_queue")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let known_lemmas: std::collections::BTreeSet<String> =
        known.iter().map(|(lemma, _)| fold_lemma(lemma)).collect();

    let mut opened = 0usize;
    for lemma in present.difference(&known_lemmas) {
        ctx.tx.execute(
            "INSERT INTO oos_queue (oos_lemma, status, first_seen) VALUES (?1, 'open', ?2)
             ON CONFLICT (oos_lemma) DO NOTHING",
            rusqlite::params![lemma, ctx.now],
        )?;
        ctx.event(
            EventDraft::new(EntityType::OosQueue, lemma.clone(), Action::OosOpened)
                .detail(serde_json::json!({ "lemma": lemma })),
        )?;
        ctx.touch(EntityType::OosQueue, lemma.clone());
        opened += 1;
    }

    let mut closed = 0usize;
    for (lemma, status) in &known {
        if status != OosStatus::Open.as_str() {
            continue;
        }
        if present.contains(&fold_lemma(lemma)) {
            continue;
        }
        ctx.tx.execute(
            "UPDATE oos_queue SET status = 'auto_closed', resolved_at = ?2, resolved_by = 'reconciler'
             WHERE oos_lemma = ?1",
            rusqlite::params![lemma, ctx.now],
        )?;
        ctx.event(
            EventDraft::new(EntityType::OosQueue, lemma.clone(), Action::OosAutoClosed)
                .detail(serde_json::json!({ "lemma": lemma })),
        )?;
        ctx.touch(EntityType::OosQueue, lemma.clone());
        closed += 1;
    }

    Ok(WriteResult::OosSync { opened, closed })
}

pub(super) fn resolve_oov(
    lemma: &str,
    resolution: OovResolution,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    let lemma = fold_lemma(lemma);
    if lemma.is_empty() {
        return Err(StoreError::invalid("oos lemma must not be empty"));
    }

    let (status, result) = match resolution {
        OovResolution::Promote {
            phonetic,
            frequency_rank,
        } => {
            let existing: Option<(i64, String, Option<String>)> = ctx
                .tx
                .query_row(
                    "SELECT word_id, role, aux_status FROM words WHERE lemma = ?1",
                    rusqlite::params![lemma],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;

            let result = match existing {
                None => words::create_word(
                    CreateWord {
                        lemma: lemma.clone(),
                        role: Role::Auxiliary,
                        phonetic,
                        frequency_rank,
                        created_by: CreatedBy::Promotion,
                        if_absent: true,
                    },
                    ctx,
                )?,
                Some((word_id, role, aux_status)) => {
                    // A retired auxiliary coming back into use is reactivated;
                    // an existing target/base word needs no promotion at all.
                    if role == Role::Auxiliary.as_str()
                        && aux_status.as_deref() != Some(AuxStatus::Active.as_str())
                    {
                        ctx.tx.execute(
                            "UPDATE words SET aux_status = 'active' WHERE word_id = ?1",
                            rusqlite::params![word_id],
                        )?;
                        ctx.event(
                            EventDraft::new(
                                EntityType::Word,
                                word_id.to_string(),
                                Action::AuxPromoted,
                            )
                            .detail(serde_json::json!({ "lemma": lemma, "reactivated": true })),
                        )?;
                        ctx.touch(EntityType::Word, word_id.to_string());
                    }
                    WriteResult::Word {
                        word_id,
                        created: false,
                    }
                }
            };
            (OosStatus::ResolvedPromote, result)
        }
        OovResolution::Rewrite {
            def_cand_id,
            text,
            source,
        } => {
            let (word_id, pos): (i64, String) = ctx
                .tx
                .query_row(
                    "SELECT word_id, pos FROM definition_candidates WHERE def_cand_id = ?1",
                    rusqlite::params![def_cand_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| {
                    StoreError::not_found(format!("definition candidate {def_cand_id}"))
                })?;

            let minted = selections::mint_definition_candidate(
                MintDefinitionCandidate {
                    word_id,
                    pos: pos.clone(),
                    text,
                    source,
                    source_ref: Some(format!("oos_rewrite:{lemma}")),
                    parent_cand_id: Some(def_cand_id),
                    created_by: None,
                    select: false,
                },
                ctx,
            )?;
            let new_cand_id = minted
                .def_cand_id()
                .ok_or_else(|| StoreError::invalid("mint did not return a candidate id"))?;
            selections::set_selection(
                SetSelection {
                    slot: morpho_domain::types::SlotRef::Definition { word_id, pos },
                    cand_id: new_cand_id,
                    selected_by: SelectedBy::Human,
                    pinned: true,
                },
                ctx,
            )?;
            (OosStatus::ResolvedRewrite, minted)
        }
        OovResolution::Gloss { zh_gloss, source } => {
            if morpho_domain::canon::canonicalize(&zh_gloss).is_empty() {
                return Err(StoreError::invalid("zh_gloss must not be empty"));
            }
            let created = words::create_word(
                CreateWord {
                    lemma: lemma.clone(),
                    role: Role::Auxiliary,
                    phonetic: None,
                    frequency_rank: None,
                    created_by: CreatedBy::Promotion,
                    if_absent: true,
                },
                ctx,
            )?;
            let word_id = created
                .word_id()
                .ok_or_else(|| StoreError::invalid("gloss resolve did not return a word id"))?;
            // A retired auxiliary that is being anchored comes back: an anchor
            // has to exist for a definition token to resolve to it.
            ctx.tx.execute(
                "UPDATE words SET aux_status = 'active'
                 WHERE word_id = ?1 AND role = 'auxiliary' AND aux_status <> 'active'",
                rusqlite::params![word_id],
            )?;
            words::set_gloss(
                SetGloss {
                    word_id,
                    zh_gloss: Some(zh_gloss),
                    source,
                },
                ctx,
            )?;
            (OosStatus::ResolvedGloss, created)
        }
    };

    ctx.tx.execute(
        "INSERT INTO oos_queue (oos_lemma, status, first_seen, resolved_by, resolved_at)
         VALUES (?1, ?2, ?3, ?4, ?3)
         ON CONFLICT (oos_lemma) DO UPDATE SET
             status = excluded.status,
             resolved_by = excluded.resolved_by,
             resolved_at = excluded.resolved_at",
        rusqlite::params![lemma, status.as_str(), ctx.now, ctx.actor.to_string()],
    )?;
    ctx.event(
        EventDraft::new(EntityType::OosQueue, lemma.clone(), Action::OosResolved)
            .detail(serde_json::json!({ "lemma": lemma, "status": status.as_str() })),
    )?;
    ctx.touch(EntityType::OosQueue, lemma);

    Ok(result)
}
