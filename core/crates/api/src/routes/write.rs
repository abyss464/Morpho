//! Mutation endpoints.
//!
//! Each one is a thin translation into a single [`WriteOp`]: that is what makes
//! every edit atomically audited and automatically visible to the reconciler
//! through the change bus. There is no second channel between the admin UI and
//! the engine (README Part 4 §"组件").

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;

use morpho_domain::types::{CandidateKind, DefinitionSource, SelectedBy, SlotRef};
use morpho_store::ops::{MintDefinitionCandidate, OovResolution, SetApproval, SetSelection};
use morpho_store::WriteOp;

use crate::dto::*;
use crate::error::{ApiError, ApiResult};
use crate::queries;
use crate::state::AppState;

/// Reload the affected word so every mutation returns the updated resource.
async fn word_detail(state: &AppState, word_id: i64) -> ApiResult<Json<WordDetail>> {
    let detail = state
        .store
        .read(move |conn| queries::word_detail(conn, word_id))
        .await?;
    Ok(Json(detail))
}

fn slot_ref(
    kind: CandidateKind,
    word_id: i64,
    pos: Option<&str>,
    slot: Option<i64>,
) -> ApiResult<SlotRef> {
    match kind {
        CandidateKind::Definition => {
            let pos = pos.ok_or_else(|| ApiError::bad_request("definition slots require `pos`"))?;
            Ok(SlotRef::Definition {
                word_id,
                pos: pos.to_string(),
            })
        }
        CandidateKind::Example => {
            let slot = slot.ok_or_else(|| ApiError::bad_request("example slots require `slot`"))?;
            if !(1..=3).contains(&slot) {
                return Err(ApiError::bad_request("example slot must be 1, 2 or 3"));
            }
            Ok(SlotRef::Example { word_id, slot })
        }
        CandidateKind::Image => Ok(SlotRef::Image { word_id }),
    }
}

fn parse_kind(raw: &str) -> ApiResult<CandidateKind> {
    raw.parse::<CandidateKind>()
        .map_err(|_| ApiError::bad_request(format!("unknown kind {raw:?}")))
}

/// `POST /api/candidates/definition`
pub async fn mint_definition(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MintDefinitionBody>,
) -> ApiResult<(StatusCode, Json<DefinitionCandidateDto>)> {
    let actor = state.actor(&headers);
    let word_id = body.word_id;
    let outcome = state
        .store
        .write(
            actor,
            WriteOp::MintDefinitionCandidate(MintDefinitionCandidate {
                word_id,
                pos: body.pos,
                text: body.text,
                // A candidate arriving through the admin API is human-authored.
                source: DefinitionSource::Manual,
                source_ref: None,
                parent_cand_id: body.parent_cand_id,
                created_by: None,
                select: false,
            }),
        )
        .await?;
    let def_cand_id = outcome
        .result
        .def_cand_id()
        .ok_or_else(|| ApiError::internal("mint did not return a candidate id"))?;

    let detail = state
        .store
        .read(move |conn| queries::word_detail(conn, word_id))
        .await?;
    let candidate = detail
        .definitions
        .into_iter()
        .flat_map(|slot| slot.candidates)
        .find(|c| c.def_cand_id == def_cand_id)
        .ok_or_else(|| ApiError::internal("minted candidate disappeared"))?;
    Ok((StatusCode::CREATED, Json(candidate)))
}

/// `POST /api/candidates/{kind}/{cand_id}/reject`
pub async fn reject_candidate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((kind, cand_id)): Path<(String, i64)>,
) -> ApiResult<Json<WordDetail>> {
    let kind = parse_kind(&kind)?;
    let actor = state.actor(&headers);
    let word_id = candidate_word_id(&state, kind, cand_id).await?;
    state
        .store
        .write(actor, WriteOp::RejectCandidate { kind, cand_id })
        .await?;
    word_detail(&state, word_id).await
}

async fn candidate_word_id(state: &AppState, kind: CandidateKind, cand_id: i64) -> ApiResult<i64> {
    let (table, pk) = match kind {
        CandidateKind::Definition => ("definition_candidates", "def_cand_id"),
        CandidateKind::Example => ("example_candidates", "ex_cand_id"),
        CandidateKind::Image => ("image_candidates", "img_cand_id"),
    };
    let sql = format!("SELECT word_id FROM {table} WHERE {pk} = ?1");
    let found: Option<i64> = state
        .store
        .read(move |conn| {
            use rusqlite::OptionalExtension;
            Ok(conn
                .query_row(&sql, rusqlite::params![cand_id], |row| row.get(0))
                .optional()?)
        })
        .await?;
    found.ok_or_else(|| ApiError::not_found(format!("{kind} candidate {cand_id}")))
}

/// `POST /api/selections/{kind}`
pub async fn set_selection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(kind): Path<String>,
    Json(body): Json<SelectionBody>,
) -> ApiResult<Json<WordDetail>> {
    let kind = parse_kind(&kind)?;
    let slot = slot_ref(kind, body.word_id, body.pos.as_deref(), body.slot)?;
    let actor = state.actor(&headers);
    state
        .store
        .write(
            actor,
            WriteOp::SetSelection(SetSelection {
                slot,
                cand_id: body.cand_id,
                // A human override always pins (README Part 3 §"人工覆盖").
                selected_by: SelectedBy::Human,
                pinned: true,
            }),
        )
        .await?;
    word_detail(&state, body.word_id).await
}

/// `POST /api/selections/{kind}/approve`
pub async fn approve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(kind): Path<String>,
    Json(body): Json<ApprovalBody>,
) -> ApiResult<Json<WordDetail>> {
    set_approval(state, headers, kind, body, true).await
}

/// `DELETE /api/selections/{kind}/approve`
pub async fn unapprove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(kind): Path<String>,
    Json(body): Json<ApprovalBody>,
) -> ApiResult<Json<WordDetail>> {
    set_approval(state, headers, kind, body, false).await
}

async fn set_approval(
    state: AppState,
    headers: HeaderMap,
    kind: String,
    body: ApprovalBody,
    approved: bool,
) -> ApiResult<Json<WordDetail>> {
    let kind = parse_kind(&kind)?;
    let slot = slot_ref(kind, body.word_id, body.pos.as_deref(), body.slot)?;
    let actor = state.actor(&headers);
    state
        .store
        .write(actor, WriteOp::SetApproval(SetApproval { slot, approved }))
        .await?;
    word_detail(&state, body.word_id).await
}

/// `POST /api/oov/{lemma}/resolve`
pub async fn resolve_oov(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(lemma): Path<String>,
    Json(body): Json<OovResolveBody>,
) -> ApiResult<Json<OovResolveResponse>> {
    let actor = state.actor(&headers);
    let (resolution, status) = match body {
        OovResolveBody::Promote {
            phonetic,
            frequency_rank,
        } => (
            OovResolution::Promote {
                phonetic,
                frequency_rank,
            },
            "resolved_promote",
        ),
        OovResolveBody::Rewrite { def_cand_id, text } => (
            OovResolution::Rewrite {
                def_cand_id,
                text,
                // Rewrites typed by a person, not drafted by the LLM executor.
                source: DefinitionSource::Manual,
            },
            "resolved_rewrite",
        ),
    };

    let outcome = state
        .store
        .write(
            actor,
            WriteOp::ResolveOov {
                lemma: lemma.clone(),
                resolution,
            },
        )
        .await?;

    Ok(Json(OovResolveResponse {
        oos_lemma: morpho_domain::fold_lemma(&lemma),
        status: status.to_string(),
        word_id: outcome.result.word_id(),
        def_cand_id: outcome.result.def_cand_id(),
    }))
}
