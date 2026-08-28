//! Mutation endpoints.
//!
//! Each one is a thin translation into a single [`WriteOp`]: that is what makes
//! every edit atomically audited and automatically visible to the reconciler
//! through the change bus. There is no second channel between the admin UI and
//! the engine (README Part 4 §"组件").
//!
//! Word-scoped mutations answer with the whole [`WordDetail`] (wave-2 ruling
//! #2): one edit cascades through selection, approval and readiness, so the row
//! alone would be useless to the console.

use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;

use morpho_domain::types::{
    CandidateKind, CreatedBy, DefinitionSource, ExampleSource, GlossSource, ImageSource, MediaKind,
    Role, SelectedBy, SlotRef,
};
use morpho_store::ops::{
    CreateWord, DistractorRebind, MediaRegistration, MintDefinitionCandidate, MintExampleCandidate,
    MintImageCandidate, OovResolution, RebindDistractors, SetApproval, SetGloss, SetSelection,
};
use morpho_store::WriteOp;

use crate::dto::*;
use crate::error::{ApiError, ApiResult};
use crate::queries;
use crate::state::AppState;

/// Largest image upload accepted. A 768×576 WebP is tens of kilobytes; this is
/// generous for a source photograph and still bounded.
const MAX_UPLOAD_BYTES: usize = 24 * 1024 * 1024;

/// Reload the affected word so every mutation returns the updated resource.
pub(crate) async fn load_word(state: &AppState, word_id: i64) -> ApiResult<WordDetail> {
    let tts = state.tts.clone();
    Ok(state
        .store
        .read(move |conn| queries::word_detail(conn, word_id, &tts))
        .await?)
}

async fn word_response(state: &AppState, word_id: i64) -> ApiResult<Json<WordDetail>> {
    Ok(Json(load_word(state, word_id).await?))
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

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

/// `POST /api/words`
pub async fn create_word(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateWordBody>,
) -> ApiResult<(StatusCode, Json<WordDetail>)> {
    let role = body
        .role
        .parse::<Role>()
        .map_err(|_| ApiError::bad_request(format!("unknown role {:?}", body.role)))?;
    let actor = state.actor(&headers);
    let outcome = state
        .store
        .write(
            actor,
            WriteOp::CreateWord(CreateWord {
                lemma: body.lemma,
                role,
                phonetic: body.phonetic,
                frequency_rank: body.frequency_rank,
                created_by: CreatedBy::Manual,
                if_absent: false,
            }),
        )
        .await?;
    let word_id = outcome
        .result
        .word_id()
        .ok_or_else(|| ApiError::internal("word creation did not return an id"))?;
    Ok((StatusCode::CREATED, Json(load_word(&state, word_id).await?)))
}

/// `POST /api/words/{id}/gloss`
///
/// Anchoring a word is the operator's answer to "this word is referenced but
/// will never be learnable": the gloss terminates the readability chain, and
/// the word leaves the curriculum without anything being deleted
/// (admin-api.md ruling #18a).
pub async fn set_gloss(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(word_id): Path<i64>,
    Json(body): Json<SetGlossBody>,
) -> ApiResult<Json<WordDetail>> {
    if body.zh_gloss.trim().is_empty() {
        return Err(ApiError::bad_request("zh_gloss must not be empty"));
    }
    let actor = state.actor(&headers);
    state
        .store
        .write(
            actor,
            WriteOp::SetGloss(SetGloss {
                word_id,
                zh_gloss: Some(body.zh_gloss),
                // A gloss typed into the console is manual; `cedict` is for a
                // future bulk import that reads a dictionary file.
                source: GlossSource::Manual,
            }),
        )
        .await?;
    word_response(&state, word_id).await
}

/// `DELETE /api/words/{id}/gloss` — the word resumes normal life.
pub async fn clear_gloss(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(word_id): Path<i64>,
) -> ApiResult<Json<WordDetail>> {
    let actor = state.actor(&headers);
    state
        .store
        .write(
            actor,
            WriteOp::SetGloss(SetGloss {
                word_id,
                zh_gloss: None,
                source: GlossSource::Manual,
            }),
        )
        .await?;
    word_response(&state, word_id).await
}

// ---------------------------------------------------------------------------
// Candidates
// ---------------------------------------------------------------------------

/// `POST /api/candidates/definition`
pub async fn mint_definition(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MintDefinitionBody>,
) -> ApiResult<(StatusCode, Json<WordDetail>)> {
    let actor = state.actor(&headers);
    let word_id = body.word_id;
    state
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
    Ok((StatusCode::CREATED, Json(load_word(&state, word_id).await?)))
}

/// `POST /api/candidates/example`
pub async fn mint_example(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MintExampleBody>,
) -> ApiResult<(StatusCode, Json<WordDetail>)> {
    let actor = state.actor(&headers);
    let word_id = body.word_id;
    state
        .store
        .write(
            actor,
            WriteOp::MintExampleCandidate(MintExampleCandidate {
                word_id,
                text: body.text,
                hl_start: body.hl_start,
                hl_end: body.hl_end,
                source: ExampleSource::Manual,
                source_ref: None,
                created_by: None,
            }),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(load_word(&state, word_id).await?)))
}

/// `POST /api/candidates/image` — multipart upload.
///
/// The bytes go through the same encoder as a fetched photograph, so a manual
/// upload and a stock photo are indistinguishable downstream: fitted to the
/// target box, WebP, content-addressed.
pub async fn upload_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<WordDetail>)> {
    let mut word_id: Option<i64> = None;
    let mut bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::bad_request(format!("malformed upload: {err}")))?
    {
        match field.name() {
            Some("word_id") => {
                let raw = field
                    .text()
                    .await
                    .map_err(|err| ApiError::bad_request(format!("bad word_id: {err}")))?;
                word_id = Some(
                    raw.trim()
                        .parse()
                        .map_err(|_| ApiError::bad_request("word_id must be an integer"))?,
                );
            }
            Some("file") => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|err| ApiError::bad_request(format!("bad file: {err}")))?;
                if data.len() > MAX_UPLOAD_BYTES {
                    return Err(ApiError::bad_request(format!(
                        "upload is {} bytes, over the {MAX_UPLOAD_BYTES}-byte limit",
                        data.len()
                    )));
                }
                bytes = Some(data.to_vec());
            }
            _ => {}
        }
    }

    let word_id = word_id.ok_or_else(|| ApiError::bad_request("word_id is required"))?;
    let bytes = bytes.ok_or_else(|| ApiError::bad_request("file is required"))?;

    let encoded = morpho_reconcile::sources::images::encode(&bytes)
        .map_err(|err| ApiError::bad_request(format!("unusable image: {}", err.message())))?;
    let stored = state
        .media
        .put_bytes(&encoded.bytes, MediaKind::Image)
        .map_err(|err| ApiError::internal(err.to_string()))?;

    let actor = state.actor(&headers);
    let user = state.user(&headers);
    state
        .store
        .write(
            actor,
            WriteOp::MintImageCandidate(MintImageCandidate {
                word_id,
                pos: None,
                file_hash: stored.file_hash.clone(),
                media: Some(MediaRegistration {
                    file_hash: stored.file_hash,
                    kind: MediaKind::Image,
                    rel_path: stored.rel_path,
                    bytes: stored.bytes,
                }),
                width: Some(i64::from(encoded.width)),
                height: Some(i64::from(encoded.height)),
                source: ImageSource::Manual,
                source_ref: Some(format!("upload by {user}")),
                license: None,
                query_used: None,
                created_by: None,
            }),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(load_word(&state, word_id).await?)))
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
    word_response(&state, word_id).await
}

/// `DELETE /api/candidates/example/{cand_id}`
///
/// Erasure, as opposed to rejection: the row goes. Rejection is what the engine
/// and the console use — reversible, auditable, and it leaves the sentence
/// visible in the candidate strip — so this is for content that should never
/// have been minted at all. A candidate a slot still points at is refused;
/// move the slot first.
pub async fn purge_example(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cand_id): Path<i64>,
) -> ApiResult<Json<WordDetail>> {
    let actor = state.actor(&headers);
    let word_id = candidate_word_id(&state, CandidateKind::Example, cand_id).await?;
    state
        .store
        .write(
            actor,
            WriteOp::PurgeExampleCandidate {
                ex_cand_id: cand_id,
            },
        )
        .await?;
    word_response(&state, word_id).await
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

// ---------------------------------------------------------------------------
// Selections
// ---------------------------------------------------------------------------

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
                // A human override pins by default (README Part 3 §"人工覆盖");
                // an automated caller that does not want to lock the slot
                // passes `pin: false` (e.g. ops/verify_genimg.py, which selects
                // a CLIP-verified upload but leaves it open to a later,
                // better-scoring candidate).
                selected_by: SelectedBy::Human,
                pinned: body.pin,
            }),
        )
        .await?;
    word_response(&state, body.word_id).await
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
    word_response(&state, body.word_id).await
}

/// `POST /api/selections/definition/primary`
pub async fn set_primary(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SetPrimaryBody>,
) -> ApiResult<Json<WordDetail>> {
    let actor = state.actor(&headers);
    state
        .store
        .write(
            actor,
            WriteOp::SetPrimarySense {
                word_id: body.word_id,
                pos: body.pos,
            },
        )
        .await?;
    word_response(&state, body.word_id).await
}

/// `POST /api/selections/definition/enabled`
pub async fn set_enabled(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SetEnabledBody>,
) -> ApiResult<Json<WordDetail>> {
    let actor = state.actor(&headers);
    state
        .store
        .write(
            actor,
            WriteOp::SetSlotEnabled {
                word_id: body.word_id,
                pos: body.pos,
                enabled: body.enabled,
            },
        )
        .await?;
    word_response(&state, body.word_id).await
}

// ---------------------------------------------------------------------------
// OOV queue
// ---------------------------------------------------------------------------

/// `POST /api/oov/{lemma}/resolve`
pub async fn resolve_oov(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(lemma): Path<String>,
    Json(body): Json<OovResolveBody>,
) -> ApiResult<Json<Page<OovQueueEntry>>> {
    let actor = state.actor(&headers);
    let (resolution, notes) = match body {
        OovResolveBody::Promote {
            notes,
            phonetic,
            frequency_rank,
        } => (
            OovResolution::Promote {
                phonetic,
                frequency_rank,
            },
            notes,
        ),
        OovResolveBody::Rewrite {
            def_cand_id,
            text,
            notes,
        } => (
            OovResolution::Rewrite {
                def_cand_id,
                text,
                // Rewrites typed by a person, not drafted by the LLM executor.
                source: DefinitionSource::Manual,
            },
            notes,
        ),
        OovResolveBody::Gloss { zh_gloss, notes } => {
            if zh_gloss.trim().is_empty() {
                return Err(ApiError::bad_request("zh_gloss must not be empty"));
            }
            (
                OovResolution::Gloss {
                    zh_gloss,
                    source: GlossSource::Manual,
                },
                notes,
            )
        }
    };

    let folded = morpho_domain::fold_lemma(&lemma);
    let mut ops = vec![WriteOp::ResolveOov {
        lemma: lemma.clone(),
        resolution,
    }];
    if let Some(notes) = notes {
        ops.push(WriteOp::AppendEvent {
            entity_type: morpho_domain::EntityType::OosQueue,
            entity_id: folded.clone(),
            action: morpho_domain::Action::OosResolved,
            detail: Some(serde_json::json!({ "notes": notes })),
        });
    }
    state.store.write(actor, WriteOp::Batch(ops)).await?;

    // Answer with the queue the console is looking at, so one resolve refreshes
    // the whole page.
    let query = OovQuery {
        status: Some("open".to_string()),
        page: None,
        page_size: None,
    };
    let body = state
        .store
        .read(move |conn| queries::oov_page(conn, &query))
        .await?;
    Ok(Json(body))
}

// ---------------------------------------------------------------------------
// Distractors
// ---------------------------------------------------------------------------

/// Most bindings a single call will report per array. The plan itself is
/// unbounded — every replacement it writes gets its own `events` row — but a
/// console does not need ten thousand rows in one response to decide whether
/// to run it for real.
const MAX_REBIND_ITEMS: usize = 500;

/// `POST /api/distractors/rebind-violations`
///
/// Distractors are bound once and never change (README Part 3), with one
/// exception: a human replacing a binding that should never have been made.
/// Rows written before the stem-exclusion rule existed still pair a word with
/// its own derivation — `adapt`/`adapter`, `invest`/`investor` — which teaches
/// the elimination pattern instead of the word. This finds them and offers the
/// replacement the current rule would have chosen.
///
/// `dry_run` (default `true`) computes the whole plan and writes nothing.
pub async fn rebind_violations(
    State(state): State<AppState>,
    headers: HeaderMap,
    // The two `Option`s cover the two ways to say nothing: no body at all
    // (`curl -X POST`), and a literal `null` payload. Both mean "dry run".
    body: Option<Json<Option<RebindViolationsBody>>>,
) -> ApiResult<Json<RebindReport>> {
    let dry_run = body.and_then(|Json(body)| body).unwrap_or_default().dry_run;

    // One snapshot: the plan must be computed against a single consistent read,
    // or two ranks of the same word could be planned against different pools.
    let (pairs, plans) = state
        .store
        .read(|conn| {
            let pairs = morpho_store::queries::distractor_pairs(conn)?;
            let pool = morpho_store::queries::active_words(conn)?;

            let mut pos_ctx = morpho_reconcile::stages::PosContext::default();
            for (word_id, pos) in morpho_store::queries::primary_pos_map(conn)? {
                pos_ctx.primary.insert(word_id, pos);
            }
            for (word_id, pos) in morpho_store::queries::word_pos_set(conn)? {
                pos_ctx.all.entry(word_id).or_default().insert(pos);
            }

            let plans = morpho_reconcile::stages::plan_stem_rebinds(&pairs, &pool, &pos_ctx);
            Ok((pairs.len(), plans))
        })
        .await?;

    let (resolved, unresolvable): (Vec<_>, Vec<_>) = plans
        .into_iter()
        .partition(morpho_reconcile::stages::RebindPlan::is_resolved);

    let mut skipped = 0usize;
    if !dry_run && !resolved.is_empty() {
        let rebinds: Vec<DistractorRebind> = resolved
            .iter()
            .filter_map(|plan| {
                Some(DistractorRebind {
                    word_id: plan.word_id,
                    rank: plan.rank,
                    old_distractor_word_id: plan.old_word_id,
                    new_distractor_word_id: plan.new_word_id?,
                })
            })
            .collect();
        let actor = state.actor(&headers);
        let outcome = state
            .store
            .write(
                actor,
                WriteOp::RebindDistractors(RebindDistractors {
                    rebinds,
                    algo_ver: morpho_domain::version::DISTRACTOR_ALGO_VER.to_string(),
                    reason: morpho_reconcile::stages::STEM_VIOLATION_REASON.to_string(),
                }),
            )
            .await?;
        if let morpho_store::WriteResult::Rebound { skipped: count, .. } = outcome.result {
            skipped = count;
        }
    }

    let planned_or_applied_total = resolved.len();
    let unresolvable_total = unresolvable.len();
    Ok(Json(RebindReport {
        scanned: pairs,
        violations: planned_or_applied_total + unresolvable_total,
        planned_or_applied: resolved
            .iter()
            .take(MAX_REBIND_ITEMS)
            .map(rebind_item)
            .collect(),
        unresolvable: unresolvable
            .iter()
            .take(MAX_REBIND_ITEMS)
            .map(rebind_item)
            .collect(),
        truncated: planned_or_applied_total > MAX_REBIND_ITEMS
            || unresolvable_total > MAX_REBIND_ITEMS,
        planned_or_applied_total,
        unresolvable_total,
        applied: !dry_run,
        skipped,
    }))
}

fn rebind_item(plan: &morpho_reconcile::stages::RebindPlan) -> RebindItem {
    RebindItem {
        word_id: plan.word_id,
        lemma: plan.lemma.clone(),
        rank: plan.rank,
        old: RebindWordRef {
            word_id: plan.old_word_id,
            lemma: plan.old_lemma.clone(),
        },
        new: plan.new_word_id.map(|word_id| RebindWordRef {
            word_id,
            lemma: plan.new_lemma.clone().unwrap_or_default(),
        }),
        core_ready_new: plan.core_ready_new,
    }
}

// ---------------------------------------------------------------------------
// Dead letters
// ---------------------------------------------------------------------------

/// `POST /api/dead-letters/retry` — deleting the row *is* the retry.
pub async fn retry_dead_letter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<JobKeyBody>,
) -> ApiResult<Json<Page<DeadLetter>>> {
    let key = job_key(&body)?;
    let actor = state.actor(&headers);
    state
        .store
        .write(
            actor,
            WriteOp::Batch(vec![
                WriteOp::ClearJobState { key: key.clone() },
                WriteOp::AppendEvent {
                    entity_type: morpho_domain::EntityType::JobState,
                    entity_id: format!(
                        "{}:{}:{}",
                        key.kind, key.subject.subject_type, key.subject.subject_id
                    ),
                    action: morpho_domain::Action::JobRetried,
                    detail: Some(serde_json::json!({
                        "kind": key.kind.as_str(),
                        "subject_type": key.subject.subject_type.as_str(),
                        "subject_id": key.subject.subject_id,
                    })),
                },
            ]),
        )
        .await?;
    dead_letter_page(&state).await
}

/// `POST /api/dead-letters/waive` — "this need is permanently satisfied by
/// absence", which is exactly what unblocks a fallback rule.
pub async fn waive_dead_letter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<JobKeyBody>,
) -> ApiResult<Json<Page<DeadLetter>>> {
    let key = job_key(&body)?;
    let actor = state.actor(&headers);
    let existing = state
        .store
        .read({
            let key = key.clone();
            move |conn| {
                use rusqlite::OptionalExtension;
                Ok(conn
                    .query_row(
                        "SELECT rate_key, attempts, last_error FROM job_state
                         WHERE kind = ?1 AND subject_type = ?2 AND subject_id = ?3",
                        rusqlite::params![
                            key.kind.as_str(),
                            key.subject.subject_type.as_str(),
                            key.subject.subject_id
                        ],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, Option<String>>(2)?,
                            ))
                        },
                    )
                    .optional()?)
            }
        })
        .await?;
    let (rate_key, attempts, last_error) =
        existing.ok_or_else(|| ApiError::not_found(format!("no job_state row for {key}")))?;
    let rate_key = rate_key
        .parse()
        .unwrap_or_else(|_| key.kind.default_rate_key());

    state
        .store
        .write(
            actor,
            WriteOp::UpsertJobState(morpho_store::ops::UpsertJobState {
                key,
                rate_key,
                status: morpho_domain::JobStatus::Waived,
                attempts,
                next_retry_at: None,
                last_error,
            }),
        )
        .await?;
    dead_letter_page(&state).await
}

async fn dead_letter_page(state: &AppState) -> ApiResult<Json<Page<DeadLetter>>> {
    let body = state
        .store
        .read(move |conn| queries::dead_letters(conn, Pagination::default(), None))
        .await?;
    Ok(Json(body))
}

fn job_key(body: &JobKeyBody) -> ApiResult<morpho_domain::JobKey> {
    let kind = body
        .kind
        .parse::<morpho_domain::JobKind>()
        .map_err(|_| ApiError::bad_request(format!("unknown job kind {:?}", body.kind)))?;
    let subject_type = body
        .subject_type
        .parse::<morpho_domain::SubjectType>()
        .map_err(|_| {
            ApiError::bad_request(format!("unknown subject type {:?}", body.subject_type))
        })?;
    Ok(morpho_domain::JobKey::new(
        kind,
        morpho_domain::SubjectRef::new(subject_type, body.subject_id.clone()),
    ))
}

// ---------------------------------------------------------------------------
// Releases
// ---------------------------------------------------------------------------

/// `POST /api/releases/export`
pub async fn export_release(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<ExportBody>>,
) -> ApiResult<Json<serde_json::Value>> {
    let notes = body.and_then(|Json(body)| body.notes);
    let user = state.user(&headers);

    // A bundle directory is named after the moment it was requested, so two
    // exports never collide even when their content is identical.
    let out_dir = state.releases_dir.join(format!(
        "export-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ")
    ));

    match morpho_export::export(&state.store, &state.export, &out_dir, &user, notes).await {
        Ok((written, report)) => Ok(Json(serde_json::json!({
            "release": {
                "version": written.content_version,
                "input_hash": written.content_hash,
                "db_file_hash": written.db_file_hash,
                "word_count": written.word_count,
                "media_count": written.media_hashes.len(),
                "out_dir": written.out_dir.display().to_string(),
            },
            "holdback": report,
        }))),
        Err(morpho_export::ExportError::GatesFailed(failures)) => {
            Err(ApiError::export_conflict(failures))
        }
        Err(morpho_export::ExportError::NoPlan) => {
            Err(ApiError::conflict("no plan has been built yet"))
        }
        Err(err) => Err(ApiError::internal(err.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Publish
// ---------------------------------------------------------------------------

/// `POST /api/releases/publish`
///
/// Runs the full export-to-APK pipeline: export, sync release.db and media
/// into the Android project, patch the test assertion, and optionally run the
/// Gradle build. Returns the publish result.
pub async fn publish_release(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<PublishBody>>,
) -> ApiResult<Json<serde_json::Value>> {
    let (notes, no_build) = body
        .map(|Json(b)| (b.notes, b.no_build))
        .unwrap_or_default();
    let user = state.user(&headers);

    let out_dir = state.releases_dir.join(format!(
        "export-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ")
    ));

    // 1. Export
    let (written, _report) =
        match morpho_export::export(&state.store, &state.export, &out_dir, &user, notes).await {
            Ok(pair) => pair,
            Err(morpho_export::ExportError::GatesFailed(failures)) => {
                return Err(ApiError::export_conflict(failures));
            }
            Err(morpho_export::ExportError::NoPlan) => {
                return Err(ApiError::conflict("no plan has been built yet"));
            }
            Err(err) => return Err(ApiError::internal(err.to_string())),
        };

    let repo_root = state.repo_root.clone();
    let content_version = written.content_version.clone();
    let word_count = written.word_count;
    let media_count = written.media_hashes.len();
    let export_dir = written.out_dir.display().to_string();

    // Clone for the closure; the original stays for the response.
    let version_for_patch = content_version.clone();

    // 2-4. Sync + patch (blocking I/O in a spawn_blocking task)
    let sync_result =
        tokio::task::spawn_blocking(move || -> Result<Option<(String, u64)>, String> {
            // Sync release.db
            let release_db_dest = repo_root.join("app/app/src/main/assets/release.db");
            std::fs::copy(written.out_dir.join("release.db"), &release_db_dest)
                .map_err(|e| format!("copy release.db: {e}"))?;

            // Sync media
            publish_sync_media(&written, &repo_root).map_err(|e| format!("sync media: {e}"))?;

            // Patch test
            publish_patch_test(&version_for_patch, &repo_root)
                .map_err(|e| format!("patch test: {e}"))?;

            // Gradle build
            if !no_build {
                let (apk_path, apk_size) =
                    publish_gradle_build(&repo_root).map_err(|e| format!("gradle: {e}"))?;
                Ok(Some((apk_path, apk_size)))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| ApiError::internal(format!("publish task panicked: {e}")))?
        .map_err(ApiError::internal)?;

    let (apk_path, apk_size) = match sync_result {
        Some((p, s)) => (Some(p), Some(s)),
        None => (None, None),
    };

    Ok(Json(serde_json::json!({
        "version": content_version,
        "word_count": word_count,
        "media_count": media_count,
        "export_dir": export_dir,
        "apk_path": apk_path,
        "apk_size_bytes": apk_size,
    })))
}

/// Sync img/ and audio/ from the export bundle into the Android content_media
/// assets, and trash stale files via `gio trash`.
fn publish_sync_media(
    written: &morpho_export::WrittenRelease,
    repo_root: &std::path::Path,
) -> Result<(), std::io::Error> {
    let media_dest = repo_root.join("app/content_media/src/main/assets/content_media");
    std::fs::create_dir_all(&media_dest)?;

    let manifest_paths: std::collections::HashSet<String> = written
        .manifest
        .files
        .iter()
        .filter(|e| e.path.starts_with("img/") || e.path.starts_with("audio/"))
        .map(|e| e.path.clone())
        .collect();

    for rel_path in &manifest_paths {
        let src = written.out_dir.join(rel_path);
        let dst = media_dest.join(rel_path);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if dst.exists() {
            if let (Ok(sm), Ok(dm)) = (src.metadata(), dst.metadata()) {
                if sm.len() == dm.len() {
                    continue;
                }
            }
        }
        std::fs::copy(&src, &dst)?;
    }

    // Trash stale files.
    for subdir in &["img", "audio"] {
        let dir = media_dest.join(subdir);
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let rel = format!(
                "{}/{}",
                subdir,
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            if !manifest_paths.contains(&rel) {
                let _ = std::process::Command::new("gio")
                    .args(["trash", "--"])
                    .arg(&path)
                    .status();
            }
        }
    }
    Ok(())
}

/// Replace the content_version string in `ReleaseDatabaseTest.kt`.
fn publish_patch_test(
    new_version: &str,
    repo_root: &std::path::Path,
) -> Result<(), std::io::Error> {
    let test_file =
        repo_root.join("app/app/src/test/kotlin/dev/morpho/data/db/ReleaseDatabaseTest.kt");
    let content = std::fs::read_to_string(&test_file)?;

    // Match `"20YY.MM.DD+HHHHHHHH"` — the quoted content_version literal.
    // Inner: YYYY.MM.DD+HHHHHHHH = 19 chars; total with quotes = 21 bytes.
    const INNER_LEN: usize = 19;
    const TOTAL_LEN: usize = INNER_LEN + 2;
    let bytes = content.as_bytes();
    let mut i = 0;
    while i + TOTAL_LEN <= bytes.len() {
        if bytes[i] == b'"'
            && bytes[i + 1] == b'2'
            && bytes[i + 2] == b'0'
            && bytes[i + 5] == b'.'
            && bytes[i + 8] == b'.'
            && bytes[i + 11] == b'+'
            && bytes[i + TOTAL_LEN - 1] == b'"'
            && bytes[i + 3..i + 5].iter().all(|b| b.is_ascii_digit())
            && bytes[i + 6..i + 8].iter().all(|b| b.is_ascii_digit())
            && bytes[i + 9..i + 11].iter().all(|b| b.is_ascii_digit())
            && bytes[i + 12..i + 20].iter().all(|b| b.is_ascii_hexdigit())
        {
            let mut result = String::with_capacity(content.len());
            result.push_str(&content[..i]);
            result.push('"');
            result.push_str(new_version);
            result.push('"');
            result.push_str(&content[i + TOTAL_LEN..]);
            std::fs::write(&test_file, result.as_bytes())?;
            return Ok(());
        }
        i += 1;
    }
    Ok(())
}

/// Run the Gradle build and return (apk_path, apk_size_bytes).
fn publish_gradle_build(repo_root: &std::path::Path) -> Result<(String, u64), String> {
    let app_dir = repo_root.join("app");
    let status = std::process::Command::new("./gradlew")
        .args([
            ":domain:test",
            ":app:testFatApkDebugUnitTest",
            ":app:assembleFatApkDebug",
        ])
        .current_dir(&app_dir)
        .status()
        .map_err(|e| format!("spawning gradlew: {e}"))?;

    if !status.success() {
        return Err(format!("gradle build failed: {status}"));
    }

    let apk_dir = app_dir.join("app/build/outputs/apk/fatApk/debug");
    if !apk_dir.is_dir() {
        return Err(format!("APK output dir not found: {}", apk_dir.display()));
    }
    for entry in std::fs::read_dir(&apk_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("apk") {
            let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
            return Ok((path.display().to_string(), size));
        }
    }
    Err(format!("no .apk found in {}", apk_dir.display()))
}
