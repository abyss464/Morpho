//! Candidate tagging: the data-driven provenance vocabulary and the
//! many-to-many candidate↔tag assignment (#54).
//!
//! `source` is the one required, exclusive category: every image/example
//! candidate carries exactly one, and it is written on the creation path (see
//! [`assign_source_tag`], called from `candidates::mint_*`). Because that is the
//! only way a candidate is ever created, a candidate without a source cannot
//! exist — and an unknown source is refused here rather than silently coerced,
//! so a caller that omits or mistypes a source fails loudly instead of falling
//! back to a catch-all.

use rusqlite::OptionalExtension;

use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, EventDraft};
use morpho_domain::types::CandidateKind;

use super::{OpCtx, WriteResult};
use crate::error::{Result, StoreError};

/// The required provenance category. One row of it per candidate, always.
pub const SOURCE_CATEGORY: &str = "source";

/// Assign (or move) a candidate's tag in one category.
#[derive(Debug, Clone)]
pub struct AssignTag {
    pub kind: CandidateKind,
    pub cand_id: i64,
    pub category: String,
    pub value: String,
}

/// Remove a candidate's tag in one category. Refused for a required category —
/// a candidate may never be left without its source; reassign instead.
#[derive(Debug, Clone)]
pub struct UnassignTag {
    pub kind: CandidateKind,
    pub cand_id: i64,
    pub category: String,
}

/// Add one value to the vocabulary. This is how a new source is introduced —
/// an insert, never a code change.
#[derive(Debug, Clone)]
pub struct CreateTag {
    pub category: String,
    pub value: String,
    pub note: Option<String>,
}

/// Remove one value from the vocabulary. Refused while any candidate still
/// carries it (delete safety).
#[derive(Debug, Clone)]
pub struct DeleteTag {
    pub category: String,
    pub value: String,
}

/// Add a whole new tag dimension. Existing candidates simply do not carry it
/// until assigned; only a `required` category is enforced at creation time, and
/// requiredness is honoured by the creation path, so a new required category is
/// a deliberate act (nothing in-repo mints without also setting it).
#[derive(Debug, Clone)]
pub struct CreateTagCategory {
    pub category: String,
    pub required: bool,
    pub exclusive: bool,
    pub note: Option<String>,
}

/// Reject every candidate carrying one tag (the #55 bulk lever).
#[derive(Debug, Clone)]
pub struct BulkRejectByTag {
    pub category: String,
    pub value: String,
}

/// Map a candidate family to its `candidate_tag.entity_type`. Definitions are
/// not tagged (their `source` was never overloaded).
fn entity_type(kind: CandidateKind) -> Result<&'static str> {
    match kind {
        CandidateKind::Image => Ok("image"),
        CandidateKind::Example => Ok("example"),
        CandidateKind::Definition => {
            Err(StoreError::invalid("definition candidates are not tagged"))
        }
    }
}

fn entity_class(entity_type: &str) -> EntityType {
    match entity_type {
        "image" => EntityType::ImageCandidate,
        _ => EntityType::ExampleCandidate,
    }
}

/// Resolve a `(category, value)` to its tag id, or fail loudly. This is the
/// vocabulary check every assignment goes through.
fn resolve_tag(ctx: &OpCtx<'_, '_>, category: &str, value: &str) -> Result<i64> {
    ctx.tx
        .query_row(
            "SELECT tag_id FROM tag WHERE category = ?1 AND value = ?2",
            rusqlite::params![category, value],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            StoreError::invalid(format!(
                "unknown tag {category}={value:?}; add it to the vocabulary first"
            ))
        })
}

fn candidate_exists(ctx: &OpCtx<'_, '_>, entity_type: &str, cand_id: i64) -> Result<bool> {
    let table = if entity_type == "image" {
        "image_candidates"
    } else {
        "example_candidates"
    };
    let id_col = if entity_type == "image" {
        "img_cand_id"
    } else {
        "ex_cand_id"
    };
    let n: i64 = ctx.tx.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE {id_col} = ?1"),
        rusqlite::params![cand_id],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

/// Set (or move) the tag a candidate carries in one category. Returns whether
/// anything changed, so callers stay quiet on a no-op re-assertion.
///
/// The `UNIQUE (entity_type, cand_id, category)` constraint guarantees at most
/// one row per category, so this reads that one row and updates it in place
/// rather than leaning on an UPSERT whose two candidate constraints (the primary
/// key and that unique index) would fight over which one an `ON CONFLICT` names.
fn set_tag(
    ctx: &mut OpCtx<'_, '_>,
    entity_type: &str,
    cand_id: i64,
    category: &str,
    tag_id: i64,
) -> Result<bool> {
    let current: Option<i64> = ctx
        .tx
        .query_row(
            "SELECT tag_id FROM candidate_tag
             WHERE entity_type = ?1 AND cand_id = ?2 AND category = ?3",
            rusqlite::params![entity_type, cand_id, category],
            |row| row.get(0),
        )
        .optional()?;
    match current {
        Some(existing) if existing == tag_id => Ok(false),
        Some(_) => {
            ctx.tx.execute(
                "UPDATE candidate_tag SET tag_id = ?4, assigned_by = ?5, assigned_at = ?6
                 WHERE entity_type = ?1 AND cand_id = ?2 AND category = ?3",
                rusqlite::params![
                    entity_type,
                    cand_id,
                    category,
                    tag_id,
                    ctx.actor.to_string(),
                    ctx.now
                ],
            )?;
            Ok(true)
        }
        None => {
            ctx.tx.execute(
                "INSERT INTO candidate_tag
                     (entity_type, cand_id, tag_id, category, assigned_by, assigned_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    entity_type,
                    cand_id,
                    tag_id,
                    category,
                    ctx.actor.to_string(),
                    ctx.now
                ],
            )?;
            Ok(true)
        }
    }
}

/// Write a candidate's required `source` tag. Called on every creation path, so
/// that a candidate without a source is unrepresentable. `source_value` is
/// validated against the vocabulary; an unknown value aborts the transaction and
/// with it the candidate insert. No event of its own — the candidate's
/// `CandidateAdded` row already records the provenance.
pub(super) fn assign_source_tag(
    ctx: &mut OpCtx<'_, '_>,
    entity_type: &str,
    cand_id: i64,
    source_value: &str,
) -> Result<()> {
    let tag_id = resolve_tag(ctx, SOURCE_CATEGORY, source_value)?;
    set_tag(ctx, entity_type, cand_id, SOURCE_CATEGORY, tag_id)?;
    Ok(())
}

/// Drop every tag a candidate carries. Used by the erasure path (purge), which
/// removes the row the tags hang off; no foreign key does this because the
/// association is polymorphic.
pub(super) fn delete_candidate_tags(
    ctx: &mut OpCtx<'_, '_>,
    entity_type: &str,
    cand_id: i64,
) -> Result<()> {
    ctx.tx.execute(
        "DELETE FROM candidate_tag WHERE entity_type = ?1 AND cand_id = ?2",
        rusqlite::params![entity_type, cand_id],
    )?;
    Ok(())
}

/// `POST /candidates/{kind}/{id}/tags` — assign or move one tag.
pub(super) fn assign_tag(req: AssignTag, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let entity = entity_type(req.kind)?;
    if !candidate_exists(ctx, entity, req.cand_id)? {
        return Err(StoreError::not_found(format!(
            "{} candidate {}",
            entity, req.cand_id
        )));
    }
    let tag_id = resolve_tag(ctx, &req.category, &req.value)?;
    let changed = set_tag(ctx, entity, req.cand_id, &req.category, tag_id)?;
    if changed {
        ctx.event(
            EventDraft::new(
                entity_class(entity),
                req.cand_id.to_string(),
                Action::Tagged,
            )
            .detail(serde_json::json!({
                "category": req.category,
                "value": req.value,
            })),
        )?;
        ctx.touch(entity_class(entity), req.cand_id.to_string());
    }
    Ok(WriteResult::Tags {
        affected: usize::from(changed),
    })
}

/// `DELETE /candidates/{kind}/{id}/tags/{category}` — remove one tag. A required
/// category is refused: a candidate may never lose its source.
pub(super) fn unassign_tag(req: UnassignTag, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let entity = entity_type(req.kind)?;
    if category_required(ctx, &req.category)? {
        return Err(StoreError::conflict(format!(
            "category {:?} is required; reassign it rather than removing it",
            req.category
        )));
    }
    let removed = ctx.tx.execute(
        "DELETE FROM candidate_tag WHERE entity_type = ?1 AND cand_id = ?2 AND category = ?3",
        rusqlite::params![entity, req.cand_id, req.category],
    )?;
    if removed > 0 {
        ctx.event(
            EventDraft::new(
                entity_class(entity),
                req.cand_id.to_string(),
                Action::Untagged,
            )
            .detail(serde_json::json!({ "category": req.category })),
        )?;
        ctx.touch(entity_class(entity), req.cand_id.to_string());
    }
    Ok(WriteResult::Tags { affected: removed })
}

fn category_required(ctx: &OpCtx<'_, '_>, category: &str) -> Result<bool> {
    let required: Option<bool> = ctx
        .tx
        .query_row(
            "SELECT required FROM tag_category WHERE category = ?1",
            rusqlite::params![category],
            |row| row.get::<_, i64>(0).map(|v| v != 0),
        )
        .optional()?;
    required.ok_or_else(|| StoreError::not_found(format!("tag category {category:?}")))
}

/// `POST /tags/categories` — add a tag dimension.
pub(super) fn create_tag_category(
    req: CreateTagCategory,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    let inserted = ctx.tx.execute(
        "INSERT INTO tag_category (category, required, exclusive, note)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (category) DO NOTHING",
        rusqlite::params![
            req.category,
            i64::from(req.required),
            i64::from(req.exclusive),
            req.note
        ],
    )?;
    if inserted > 0 {
        ctx.event(
            EventDraft::new(EntityType::Tag, req.category.clone(), Action::TagCreated).detail(
                serde_json::json!({
                    "kind": "category",
                    "required": req.required,
                    "exclusive": req.exclusive,
                }),
            ),
        )?;
    }
    Ok(WriteResult::Tags { affected: inserted })
}

/// `POST /tags` — add one value to the vocabulary. The category must already
/// exist. Idempotent.
pub(super) fn create_tag(req: CreateTag, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let known: bool = ctx.tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM tag_category WHERE category = ?1)",
        rusqlite::params![req.category],
        |row| row.get::<_, i64>(0).map(|v| v != 0),
    )?;
    if !known {
        return Err(StoreError::invalid(format!(
            "unknown tag category {:?}; create the category first",
            req.category
        )));
    }
    let inserted = ctx.tx.execute(
        "INSERT INTO tag (category, value, note) VALUES (?1, ?2, ?3)
         ON CONFLICT (category, value) DO NOTHING",
        rusqlite::params![req.category, req.value, req.note],
    )?;
    if inserted > 0 {
        ctx.event(
            EventDraft::new(
                EntityType::Tag,
                format!("{}:{}", req.category, req.value),
                Action::TagCreated,
            )
            .detail(serde_json::json!({ "category": req.category, "value": req.value })),
        )?;
    }
    Ok(WriteResult::Tags { affected: inserted })
}

/// `DELETE /tags/{category}/{value}` — remove one value. Refused while any
/// candidate still carries it: unassign those first.
pub(super) fn delete_tag(req: DeleteTag, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let tag_id = resolve_tag(ctx, &req.category, &req.value)?;
    let refs: i64 = ctx.tx.query_row(
        "SELECT COUNT(*) FROM candidate_tag WHERE tag_id = ?1",
        rusqlite::params![tag_id],
        |row| row.get(0),
    )?;
    if refs > 0 {
        return Err(StoreError::conflict(format!(
            "tag {}={:?} is still on {refs} candidate(s); unassign them first",
            req.category, req.value
        )));
    }
    ctx.tx.execute(
        "DELETE FROM tag WHERE tag_id = ?1",
        rusqlite::params![tag_id],
    )?;
    ctx.event(
        EventDraft::new(
            EntityType::Tag,
            format!("{}:{}", req.category, req.value),
            Action::TagDeleted,
        )
        .detail(serde_json::json!({ "category": req.category, "value": req.value })),
    )?;
    Ok(WriteResult::Tags { affected: 1 })
}

/// Reject every candidate carrying one tag. Rejection (not erasure) is the
/// reversible act #55 needs; each candidate's own reject path releases the slot
/// pin and writes its audit row.
pub(super) fn bulk_reject_by_tag(
    req: BulkRejectByTag,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    let tag_id = resolve_tag(ctx, &req.category, &req.value)?;
    // Only the still-available candidates: the single-candidate reject path
    // always writes an audit row, so filtering here (rather than in the loop) is
    // what makes a re-run of the bulk reject a genuine no-op.
    let targets: Vec<(String, i64)> = {
        let mut stmt = ctx.tx.prepare(
            "SELECT 'image' AS entity_type, ic.img_cand_id AS cand_id
               FROM candidate_tag ct JOIN image_candidates ic ON ic.img_cand_id = ct.cand_id
              WHERE ct.tag_id = ?1 AND ct.entity_type = 'image' AND ic.status = 'available'
             UNION ALL
             SELECT 'example', ec.ex_cand_id
               FROM candidate_tag ct JOIN example_candidates ec ON ec.ex_cand_id = ct.cand_id
              WHERE ct.tag_id = ?1 AND ct.entity_type = 'example' AND ec.status = 'available'
              ORDER BY 1, 2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![tag_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };

    let mut rejected = 0usize;
    for (entity, cand_id) in targets {
        let kind = if entity == "image" {
            CandidateKind::Image
        } else {
            CandidateKind::Example
        };
        // Reuse the single-candidate reject path: it flips status and releases a
        // slot pinned to the candidate.
        super::selections::reject_candidate(kind, cand_id, ctx)?;
        rejected += 1;
    }
    Ok(WriteResult::Tags { affected: rejected })
}
