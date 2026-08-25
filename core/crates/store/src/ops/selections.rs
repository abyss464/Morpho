//! Candidate minting, slot selection, approval and rejection.
//!
//! Semantics follow README Part 3 §"选择语义":
//!   * candidates are immutable — an "edit" mints a new candidate;
//!   * changing which candidate a slot points at bumps `selection_rev` and
//!     invalidates approval;
//!   * approval implies a pin and records the approved content hash;
//!   * rejecting the selected candidate clears the pin and the approval so the
//!     auto-selection rule can fall back on the next cycle.

use rusqlite::OptionalExtension;

use morpho_domain::canon::canonicalize;
use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, EventDraft};
use morpho_domain::hash::text_hash;
use morpho_domain::types::{CandidateKind, CandidateStatus, DefinitionSource, SelectedBy, SlotRef};

use super::{OpCtx, WriteResult};
use crate::error::{Result, StoreError};

/// Insert an immutable definition candidate.
#[derive(Debug, Clone)]
pub struct MintDefinitionCandidate {
    pub word_id: i64,
    pub pos: String,
    pub text: String,
    pub source: DefinitionSource,
    pub source_ref: Option<String>,
    /// Rewrite lineage: points back at the candidate this one rewrites.
    pub parent_cand_id: Option<i64>,
    /// `created_by` column; defaults to the acting actor.
    pub created_by: Option<String>,
    /// Also point the slot at the new candidate (human override).
    pub select: bool,
}

/// Point one slot at one candidate.
#[derive(Debug, Clone)]
pub struct SetSelection {
    pub slot: SlotRef,
    pub cand_id: i64,
    pub selected_by: SelectedBy,
    pub pinned: bool,
}

/// Flip the approval flag of a slot's current content.
#[derive(Debug, Clone)]
pub struct SetApproval {
    pub slot: SlotRef,
    pub approved: bool,
}

#[derive(Debug, Clone)]
struct CurrentSelection {
    cand_id: i64,
    selection_rev: i64,
    approved: bool,
}

fn selection_entity(kind: CandidateKind) -> EntityType {
    match kind {
        CandidateKind::Definition => EntityType::DefinitionSelection,
        CandidateKind::Example => EntityType::ExampleSelection,
        CandidateKind::Image => EntityType::ImageSelection,
    }
}

fn candidate_entity(kind: CandidateKind) -> EntityType {
    match kind {
        CandidateKind::Definition => EntityType::DefinitionCandidate,
        CandidateKind::Example => EntityType::ExampleCandidate,
        CandidateKind::Image => EntityType::ImageCandidate,
    }
}

fn word_exists(ctx: &OpCtx<'_, '_>, word_id: i64) -> Result<bool> {
    let found: Option<i64> = ctx
        .tx
        .query_row(
            "SELECT word_id FROM words WHERE word_id = ?1",
            rusqlite::params![word_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

// ---------------------------------------------------------------------------
// Mint
// ---------------------------------------------------------------------------

pub(super) fn mint_definition_candidate(
    req: MintDefinitionCandidate,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    if !word_exists(ctx, req.word_id)? {
        return Err(StoreError::not_found(format!("word {}", req.word_id)));
    }
    let pos = canonicalize(&req.pos).to_lowercase();
    if pos.is_empty() {
        return Err(StoreError::invalid("pos must not be empty"));
    }
    // Candidate text is stored canonicalized so that `text_hash`, the TTS
    // desired-set join and the admin UI all see the same bytes.
    let text = canonicalize(&req.text);
    if text.is_empty() {
        return Err(StoreError::invalid("definition text must not be empty"));
    }
    let hash = text_hash(&text);
    let created_by = req
        .created_by
        .clone()
        .unwrap_or_else(|| ctx.actor.to_string());

    let inserted = ctx.tx.execute(
        "INSERT INTO definition_candidates
             (word_id, pos, text, text_hash, source, source_ref, parent_cand_id, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (word_id, pos, text_hash) DO NOTHING",
        rusqlite::params![
            req.word_id,
            pos,
            text,
            hash,
            req.source.as_str(),
            req.source_ref,
            req.parent_cand_id,
            created_by,
            ctx.now,
        ],
    )?;

    let def_cand_id: i64 = ctx.tx.query_row(
        "SELECT def_cand_id FROM definition_candidates
         WHERE word_id = ?1 AND pos = ?2 AND text_hash = ?3",
        rusqlite::params![req.word_id, pos, hash],
        |row| row.get(0),
    )?;

    let created = inserted > 0;
    if created {
        ctx.event(
            EventDraft::new(
                EntityType::DefinitionCandidate,
                def_cand_id.to_string(),
                Action::CandidateAdded,
            )
            .detail(serde_json::json!({
                "word_id": req.word_id,
                "pos": pos,
                "source": req.source.as_str(),
                "text_hash": hash,
                "parent_cand_id": req.parent_cand_id,
            })),
        )?;
        ctx.touch(EntityType::DefinitionCandidate, def_cand_id.to_string());
        ctx.touch(EntityType::Word, req.word_id.to_string());
    }

    if req.select {
        set_selection(
            SetSelection {
                slot: SlotRef::Definition {
                    word_id: req.word_id,
                    pos: pos.clone(),
                },
                cand_id: def_cand_id,
                selected_by: SelectedBy::Human,
                pinned: true,
            },
            ctx,
        )?;
    }

    Ok(WriteResult::DefinitionCandidate {
        def_cand_id,
        created,
    })
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

fn current_selection(ctx: &OpCtx<'_, '_>, slot: &SlotRef) -> Result<Option<CurrentSelection>> {
    let map = |row: &rusqlite::Row<'_>| {
        Ok(CurrentSelection {
            cand_id: row.get(0)?,
            selection_rev: row.get(1)?,
            approved: row.get::<_, i64>(2)? != 0,
        })
    };
    let found = match slot {
        SlotRef::Definition { word_id, pos } => ctx
            .tx
            .query_row(
                "SELECT def_cand_id, selection_rev, approved FROM definition_selections
                 WHERE word_id = ?1 AND pos = ?2",
                rusqlite::params![word_id, pos],
                map,
            )
            .optional()?,
        SlotRef::Example { word_id, slot } => ctx
            .tx
            .query_row(
                "SELECT ex_cand_id, selection_rev, approved FROM example_selections
                 WHERE word_id = ?1 AND slot = ?2",
                rusqlite::params![word_id, slot],
                map,
            )
            .optional()?,
        SlotRef::Image { word_id } => ctx
            .tx
            .query_row(
                "SELECT img_cand_id, selection_rev, approved FROM image_selections
                 WHERE word_id = ?1",
                rusqlite::params![word_id],
                map,
            )
            .optional()?,
    };
    Ok(found)
}

/// Verify the candidate exists, is available, and belongs to this slot.
fn validate_candidate(ctx: &OpCtx<'_, '_>, slot: &SlotRef, cand_id: i64) -> Result<()> {
    let (owner, status): (i64, String) = match slot {
        SlotRef::Definition { pos, .. } => {
            let row = ctx
                .tx
                .query_row(
                    "SELECT word_id, status, pos FROM definition_candidates WHERE def_cand_id = ?1",
                    rusqlite::params![cand_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| StoreError::not_found(format!("definition candidate {cand_id}")))?;
            if &row.2 != pos {
                return Err(StoreError::conflict(format!(
                    "candidate {cand_id} has pos {:?}, slot expects {pos:?}",
                    row.2
                )));
            }
            (row.0, row.1)
        }
        SlotRef::Example { .. } => ctx
            .tx
            .query_row(
                "SELECT word_id, status FROM example_candidates WHERE ex_cand_id = ?1",
                rusqlite::params![cand_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::not_found(format!("example candidate {cand_id}")))?,
        SlotRef::Image { .. } => ctx
            .tx
            .query_row(
                "SELECT word_id, status FROM image_candidates WHERE img_cand_id = ?1",
                rusqlite::params![cand_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::not_found(format!("image candidate {cand_id}")))?,
    };

    if owner != slot.word_id() {
        return Err(StoreError::conflict(format!(
            "candidate {cand_id} belongs to word {owner}, not {}",
            slot.word_id()
        )));
    }
    if status != CandidateStatus::Available.as_str() {
        return Err(StoreError::conflict(format!(
            "candidate {cand_id} is {status}"
        )));
    }
    Ok(())
}

pub(super) fn set_selection(req: SetSelection, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    validate_candidate(ctx, &req.slot, req.cand_id)?;

    if let SlotRef::Example { word_id, slot } = &req.slot {
        // UNIQUE (word_id, ex_cand_id): the same sentence cannot fill two slots.
        let other: Option<i64> = ctx
            .tx
            .query_row(
                "SELECT slot FROM example_selections WHERE word_id = ?1 AND ex_cand_id = ?2",
                rusqlite::params![word_id, req.cand_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(other_slot) = other {
            if other_slot != *slot {
                return Err(StoreError::conflict(format!(
                    "example candidate {} already fills slot {other_slot}",
                    req.cand_id
                )));
            }
        }
    }

    let previous = current_selection(ctx, &req.slot)?;
    let selected_by = req.selected_by.as_str();
    let pinned = i64::from(req.pinned);
    let entity = selection_entity(req.slot.candidate_kind());
    let entity_id = req.slot.entity_id();

    match &previous {
        Some(prev) if prev.cand_id == req.cand_id => {
            // Same content: only the flags move. Approval survives.
            let sql = match &req.slot {
                SlotRef::Definition { .. } => {
                    "UPDATE definition_selections SET selected_by = ?1, pinned = ?2, updated_at = ?3
                     WHERE word_id = ?4 AND pos = ?5"
                }
                SlotRef::Example { .. } => {
                    "UPDATE example_selections SET selected_by = ?1, pinned = ?2, updated_at = ?3
                     WHERE word_id = ?4 AND slot = ?5"
                }
                SlotRef::Image { .. } => {
                    "UPDATE image_selections SET selected_by = ?1, pinned = ?2, updated_at = ?3
                     WHERE word_id = ?4"
                }
            };
            match &req.slot {
                SlotRef::Definition { word_id, pos } => ctx.tx.execute(
                    sql,
                    rusqlite::params![selected_by, pinned, ctx.now, word_id, pos],
                )?,
                SlotRef::Example { word_id, slot } => ctx.tx.execute(
                    sql,
                    rusqlite::params![selected_by, pinned, ctx.now, word_id, slot],
                )?,
                SlotRef::Image { word_id } => ctx.tx.execute(
                    sql,
                    rusqlite::params![selected_by, pinned, ctx.now, word_id],
                )?,
            };
        }
        _ => {
            let rev = previous.as_ref().map_or(1, |p| p.selection_rev + 1);
            match &req.slot {
                SlotRef::Definition { word_id, pos } => {
                    let has_primary: bool = ctx.tx.query_row(
                        "SELECT EXISTS (SELECT 1 FROM definition_selections
                                        WHERE word_id = ?1 AND is_primary = 1)",
                        rusqlite::params![word_id],
                        |row| row.get::<_, i64>(0).map(|v| v != 0),
                    )?;
                    // README rule 5: the first selected sense of a word becomes
                    // the primary one; humans can move it later.
                    let is_primary = i64::from(previous.is_none() && !has_primary);
                    ctx.tx.execute(
                        "INSERT INTO definition_selections
                             (word_id, pos, def_cand_id, is_primary, enabled, selected_by, pinned,
                              approved, approved_hash, approved_by, approved_at, selection_rev, updated_at)
                         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, 0, NULL, NULL, NULL, ?7, ?8)
                         ON CONFLICT (word_id, pos) DO UPDATE SET
                             def_cand_id = excluded.def_cand_id,
                             selected_by = excluded.selected_by,
                             pinned = excluded.pinned,
                             approved = 0, approved_hash = NULL, approved_by = NULL, approved_at = NULL,
                             selection_rev = excluded.selection_rev,
                             updated_at = excluded.updated_at",
                        rusqlite::params![
                            word_id,
                            pos,
                            req.cand_id,
                            is_primary,
                            selected_by,
                            pinned,
                            rev,
                            ctx.now
                        ],
                    )?;
                }
                SlotRef::Example { word_id, slot } => {
                    ctx.tx.execute(
                        "INSERT INTO example_selections
                             (word_id, slot, ex_cand_id, selected_by, pinned,
                              approved, approved_hash, approved_by, approved_at, selection_rev, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, NULL, NULL, ?6, ?7)
                         ON CONFLICT (word_id, slot) DO UPDATE SET
                             ex_cand_id = excluded.ex_cand_id,
                             selected_by = excluded.selected_by,
                             pinned = excluded.pinned,
                             approved = 0, approved_hash = NULL, approved_by = NULL, approved_at = NULL,
                             selection_rev = excluded.selection_rev,
                             updated_at = excluded.updated_at",
                        rusqlite::params![
                            word_id,
                            slot,
                            req.cand_id,
                            selected_by,
                            pinned,
                            rev,
                            ctx.now
                        ],
                    )?;
                }
                SlotRef::Image { word_id } => {
                    ctx.tx.execute(
                        "INSERT INTO image_selections
                             (word_id, img_cand_id, selected_by, pinned,
                              approved, approved_hash, approved_by, approved_at, selection_rev, updated_at)
                         VALUES (?1, ?2, ?3, ?4, 0, NULL, NULL, NULL, ?5, ?6)
                         ON CONFLICT (word_id) DO UPDATE SET
                             img_cand_id = excluded.img_cand_id,
                             selected_by = excluded.selected_by,
                             pinned = excluded.pinned,
                             approved = 0, approved_hash = NULL, approved_by = NULL, approved_at = NULL,
                             selection_rev = excluded.selection_rev,
                             updated_at = excluded.updated_at",
                        rusqlite::params![word_id, req.cand_id, selected_by, pinned, rev, ctx.now],
                    )?;
                }
            }

            ctx.event(
                EventDraft::new(entity, entity_id.clone(), Action::SelectionChanged).detail(
                    serde_json::json!({
                        "slot": req.slot,
                        "from_cand_id": previous.as_ref().map(|p| p.cand_id),
                        "to_cand_id": req.cand_id,
                        "selected_by": selected_by,
                        "pinned": req.pinned,
                        "selection_rev": rev,
                    }),
                ),
            )?;

            if previous.as_ref().is_some_and(|p| p.approved) {
                ctx.event(
                    EventDraft::new(entity, entity_id.clone(), Action::ApprovalInvalidated).detail(
                        serde_json::json!({
                            "reason": "selection_changed",
                            "slot": req.slot,
                        }),
                    ),
                )?;
            }
        }
    }

    ctx.touch(entity, entity_id);
    ctx.touch(EntityType::Word, req.slot.word_id().to_string());
    Ok(WriteResult::Unit)
}

// ---------------------------------------------------------------------------
// Approval
// ---------------------------------------------------------------------------

/// Hash of the content a slot currently points at (`text_hash` / `file_hash`).
fn selected_content_hash(ctx: &OpCtx<'_, '_>, slot: &SlotRef) -> Result<String> {
    let hash: Option<String> = match slot {
        SlotRef::Definition { word_id, pos } => ctx
            .tx
            .query_row(
                "SELECT dc.text_hash FROM definition_selections ds
                 JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
                 WHERE ds.word_id = ?1 AND ds.pos = ?2",
                rusqlite::params![word_id, pos],
                |row| row.get(0),
            )
            .optional()?,
        SlotRef::Example { word_id, slot } => ctx
            .tx
            .query_row(
                "SELECT ec.text_hash FROM example_selections es
                 JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
                 WHERE es.word_id = ?1 AND es.slot = ?2",
                rusqlite::params![word_id, slot],
                |row| row.get(0),
            )
            .optional()?,
        SlotRef::Image { word_id } => ctx
            .tx
            .query_row(
                "SELECT ic.file_hash FROM image_selections isel
                 JOIN image_candidates ic ON ic.img_cand_id = isel.img_cand_id
                 WHERE isel.word_id = ?1",
                rusqlite::params![word_id],
                |row| row.get(0),
            )
            .optional()?,
    };
    hash.ok_or_else(|| StoreError::not_found(format!("selection for {slot}")))
}

pub(super) fn set_approval(req: SetApproval, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let entity = selection_entity(req.slot.candidate_kind());
    let entity_id = req.slot.entity_id();

    if req.approved {
        let hash = selected_content_hash(ctx, &req.slot)?;
        let approved_by = ctx.actor.to_string();
        // Approval implies a pin (README Part 3): auto-selection must never
        // move an approved slot underneath the approver.
        let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match &req.slot {
            SlotRef::Definition { word_id, pos } => (
                "UPDATE definition_selections
                 SET approved = 1, approved_hash = ?1, approved_by = ?2, approved_at = ?3,
                     pinned = 1, updated_at = ?3
                 WHERE word_id = ?4 AND pos = ?5",
                vec![
                    Box::new(hash.clone()),
                    Box::new(approved_by.clone()),
                    Box::new(ctx.now.clone()),
                    Box::new(*word_id),
                    Box::new(pos.clone()),
                ],
            ),
            SlotRef::Example { word_id, slot } => (
                "UPDATE example_selections
                 SET approved = 1, approved_hash = ?1, approved_by = ?2, approved_at = ?3,
                     pinned = 1, updated_at = ?3
                 WHERE word_id = ?4 AND slot = ?5",
                vec![
                    Box::new(hash.clone()),
                    Box::new(approved_by.clone()),
                    Box::new(ctx.now.clone()),
                    Box::new(*word_id),
                    Box::new(*slot),
                ],
            ),
            SlotRef::Image { word_id } => (
                "UPDATE image_selections
                 SET approved = 1, approved_hash = ?1, approved_by = ?2, approved_at = ?3,
                     pinned = 1, updated_at = ?3
                 WHERE word_id = ?4",
                vec![
                    Box::new(hash.clone()),
                    Box::new(approved_by.clone()),
                    Box::new(ctx.now.clone()),
                    Box::new(*word_id),
                ],
            ),
        };
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let changed = ctx.tx.execute(sql, param_refs.as_slice())?;
        if changed == 0 {
            return Err(StoreError::not_found(format!("selection for {}", req.slot)));
        }
        ctx.event(
            EventDraft::new(entity, entity_id.clone(), Action::Approved)
                .detail(serde_json::json!({ "slot": req.slot, "approved_hash": hash })),
        )?;
    } else {
        let changed = match &req.slot {
            SlotRef::Definition { word_id, pos } => ctx.tx.execute(
                "UPDATE definition_selections
                 SET approved = 0, approved_hash = NULL, approved_by = NULL, approved_at = NULL,
                     updated_at = ?1
                 WHERE word_id = ?2 AND pos = ?3",
                rusqlite::params![ctx.now, word_id, pos],
            )?,
            SlotRef::Example { word_id, slot } => ctx.tx.execute(
                "UPDATE example_selections
                 SET approved = 0, approved_hash = NULL, approved_by = NULL, approved_at = NULL,
                     updated_at = ?1
                 WHERE word_id = ?2 AND slot = ?3",
                rusqlite::params![ctx.now, word_id, slot],
            )?,
            SlotRef::Image { word_id } => ctx.tx.execute(
                "UPDATE image_selections
                 SET approved = 0, approved_hash = NULL, approved_by = NULL, approved_at = NULL,
                     updated_at = ?1
                 WHERE word_id = ?2",
                rusqlite::params![ctx.now, word_id],
            )?,
        };
        if changed == 0 {
            return Err(StoreError::not_found(format!("selection for {}", req.slot)));
        }
        ctx.event(
            EventDraft::new(entity, entity_id.clone(), Action::Unapproved)
                .detail(serde_json::json!({ "slot": req.slot })),
        )?;
    }

    ctx.touch(entity, entity_id);
    ctx.touch(EntityType::Word, req.slot.word_id().to_string());
    Ok(WriteResult::Unit)
}

// ---------------------------------------------------------------------------
// Rejection
// ---------------------------------------------------------------------------

pub(super) fn reject_candidate(
    kind: CandidateKind,
    cand_id: i64,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    let (table, pk) = match kind {
        CandidateKind::Definition => ("definition_candidates", "def_cand_id"),
        CandidateKind::Example => ("example_candidates", "ex_cand_id"),
        CandidateKind::Image => ("image_candidates", "img_cand_id"),
    };
    let word_id: Option<i64> = ctx
        .tx
        .query_row(
            &format!("SELECT word_id FROM {table} WHERE {pk} = ?1"),
            rusqlite::params![cand_id],
            |row| row.get(0),
        )
        .optional()?;
    let word_id =
        word_id.ok_or_else(|| StoreError::not_found(format!("{kind} candidate {cand_id}")))?;

    ctx.tx.execute(
        &format!("UPDATE {table} SET status = 'rejected' WHERE {pk} = ?1"),
        rusqlite::params![cand_id],
    )?;
    ctx.event(
        EventDraft::new(
            candidate_entity(kind),
            cand_id.to_string(),
            Action::CandidateRejected,
        )
        .detail(serde_json::json!({ "word_id": word_id, "kind": kind.as_str() })),
    )?;
    ctx.touch(candidate_entity(kind), cand_id.to_string());
    ctx.touch(EntityType::Word, word_id.to_string());

    // If the rejected candidate is currently selected, drop the pin and the
    // approval. Re-selection itself belongs to the auto-selection rule.
    let affected: Vec<(SlotRef, bool)> = match kind {
        CandidateKind::Definition => {
            let mut stmt = ctx.tx.prepare(
                "SELECT word_id, pos, approved FROM definition_selections WHERE def_cand_id = ?1",
            )?;
            let rows = stmt.query_map(rusqlite::params![cand_id], |row| {
                Ok((
                    SlotRef::Definition {
                        word_id: row.get(0)?,
                        pos: row.get(1)?,
                    },
                    row.get::<_, i64>(2)? != 0,
                ))
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        }
        CandidateKind::Example => {
            let mut stmt = ctx.tx.prepare(
                "SELECT word_id, slot, approved FROM example_selections WHERE ex_cand_id = ?1",
            )?;
            let rows = stmt.query_map(rusqlite::params![cand_id], |row| {
                Ok((
                    SlotRef::Example {
                        word_id: row.get(0)?,
                        slot: row.get(1)?,
                    },
                    row.get::<_, i64>(2)? != 0,
                ))
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        }
        CandidateKind::Image => {
            let mut stmt = ctx
                .tx
                .prepare("SELECT word_id, approved FROM image_selections WHERE img_cand_id = ?1")?;
            let rows = stmt.query_map(rusqlite::params![cand_id], |row| {
                Ok((
                    SlotRef::Image {
                        word_id: row.get(0)?,
                    },
                    row.get::<_, i64>(1)? != 0,
                ))
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        }
    };

    for (slot, was_approved) in affected {
        match &slot {
            SlotRef::Definition { word_id, pos } => ctx.tx.execute(
                "UPDATE definition_selections
                 SET pinned = 0, approved = 0, approved_hash = NULL, approved_by = NULL,
                     approved_at = NULL, updated_at = ?1
                 WHERE word_id = ?2 AND pos = ?3",
                rusqlite::params![ctx.now, word_id, pos],
            )?,
            SlotRef::Example { word_id, slot: s } => ctx.tx.execute(
                "UPDATE example_selections
                 SET pinned = 0, approved = 0, approved_hash = NULL, approved_by = NULL,
                     approved_at = NULL, updated_at = ?1
                 WHERE word_id = ?2 AND slot = ?3",
                rusqlite::params![ctx.now, word_id, s],
            )?,
            SlotRef::Image { word_id } => ctx.tx.execute(
                "UPDATE image_selections
                 SET pinned = 0, approved = 0, approved_hash = NULL, approved_by = NULL,
                     approved_at = NULL, updated_at = ?1
                 WHERE word_id = ?2",
                rusqlite::params![ctx.now, word_id],
            )?,
        };
        let entity = selection_entity(kind);
        let entity_id = slot.entity_id();
        ctx.event(
            EventDraft::new(entity, entity_id.clone(), Action::PinFallback).detail(
                serde_json::json!({ "reason": "selected_candidate_rejected", "cand_id": cand_id }),
            ),
        )?;
        if was_approved {
            ctx.event(
                EventDraft::new(entity, entity_id.clone(), Action::ApprovalInvalidated).detail(
                    serde_json::json!({ "reason": "selected_candidate_rejected", "cand_id": cand_id }),
                ),
            )?;
        }
        ctx.touch(entity, entity_id);
    }

    Ok(WriteResult::Unit)
}
