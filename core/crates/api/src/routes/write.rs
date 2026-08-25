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
    CandidateKind, CreatedBy, DefinitionSource, ExampleSource, ImageSource, MediaKind, Role,
    SelectedBy, SlotRef,
};
use morpho_store::ops::{
    CreateWord, MediaRegistration, MintDefinitionCandidate, MintExampleCandidate,
    MintImageCandidate, OovResolution, SetApproval, SetSelection,
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
                // A human override always pins (README Part 3 §"人工覆盖").
                selected_by: SelectedBy::Human,
                pinned: true,
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
        .read(move |conn| queries::dead_letters(conn, Pagination::default()))
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
