//! Read endpoints.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use morpho_domain::event::EventRecord;
use morpho_domain::job::{JobStatus, JobView, JobsSnapshot, SubjectType};
use morpho_domain::types::MediaKind;
use morpho_store::queries::job_states;

use crate::dto::*;
use crate::error::{ApiError, ApiResult};
use crate::queries;
use crate::state::AppState;

/// SSE coalescing window (wave-2 ruling #7).
const SSE_COALESCE: Duration = Duration::from_millis(250);
/// SSE keep-alive comment interval (wave-2 ruling #7).
const SSE_PING: Duration = Duration::from_secs(30);

pub async fn dashboard(State(state): State<AppState>) -> ApiResult<Json<Dashboard>> {
    let tts = state.tts.clone();
    let body = state
        .store
        .read(move |conn| queries::dashboard(conn, &tts))
        .await?;
    Ok(Json(body))
}

pub async fn events(
    State(state): State<AppState>,
    Query(query): Query<EventQuery>,
) -> ApiResult<Json<Page<EventRecord>>> {
    let body = state
        .store
        .read(move |conn| queries::events_page(conn, &query))
        .await?;
    Ok(Json(body))
}

pub async fn words(
    State(state): State<AppState>,
    Query(query): Query<WordListQuery>,
) -> ApiResult<Json<Page<WordListItem>>> {
    let tts = state.tts.clone();
    let body = state
        .store
        .read(move |conn| queries::word_list(conn, &query, &tts))
        .await?;
    Ok(Json(body))
}

pub async fn gallery(
    State(state): State<AppState>,
    Query(query): Query<GalleryQuery>,
) -> ApiResult<Json<Page<GalleryItem>>> {
    let body = state
        .store
        .read(move |conn| queries::gallery_list(conn, &query))
        .await?;
    Ok(Json(body))
}

/// `GET /api/tags` — the tag vocabulary, optionally filtered to one category.
pub async fn list_tags(
    State(state): State<AppState>,
    Query(query): Query<TagVocabQuery>,
) -> ApiResult<Json<Vec<TagEntry>>> {
    let body = state
        .store
        .read(move |conn| queries::list_tags(conn, query.category.as_deref()))
        .await?;
    Ok(Json(body))
}

/// `GET /api/tags/categories` — the tag dimensions and their flags.
pub async fn tag_categories(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<TagCategoryEntry>>> {
    let body = state.store.read(queries::list_tag_categories).await?;
    Ok(Json(body))
}

/// `GET /api/candidates/by-tag?category=&value=` — candidates carrying a tag.
pub async fn candidates_by_tag(
    State(state): State<AppState>,
    Query(query): Query<CandidatesByTagQuery>,
) -> ApiResult<Json<Page<TaggedCandidate>>> {
    let body = state
        .store
        .read(move |conn| queries::candidates_by_tag(conn, &query))
        .await?;
    Ok(Json(body))
}

pub async fn word_detail(
    State(state): State<AppState>,
    Path(word_id): Path<i64>,
) -> ApiResult<Json<WordDetail>> {
    Ok(Json(
        crate::routes::write::load_word(&state, word_id).await?,
    ))
}

pub async fn jobs(State(state): State<AppState>) -> ApiResult<Json<JobsSnapshot>> {
    // In-flight work lives only in memory; `job_state` holds the persisted
    // failure rows (backoff / dead / waived).
    let (backoff, labels) = state
        .store
        .read(|conn| {
            let labels = queries::job_labels(conn)?;
            let rows = job_states(conn)?
                .into_iter()
                .filter(|row| row.status == JobStatus::Backoff)
                .map(|row| {
                    let subject = queries::resolve_subject(
                        conn,
                        row.key.subject.subject_type.as_str(),
                        &row.key.subject.subject_id,
                    );
                    JobView {
                        kind: row.key.kind.as_str().to_string(),
                        subject_type: row.key.subject.subject_type,
                        subject_id: row.key.subject.subject_id,
                        rate_key: row.rate_key.as_str().to_string(),
                        status: Some(row.status),
                        attempts: row.attempts,
                        next_retry_at: row.next_retry_at,
                        last_error: row.last_error,
                        subject_label: Some(subject.label),
                    }
                })
                .collect::<Vec<_>>();
            Ok((rows, labels))
        })
        .await?;
    Ok(Json(state.jobs.snapshot(backoff, &labels)))
}

/// `GET /api/stream` — server-sent `ChangeEvent`s off the change bus.
///
/// Wave-2 ruling #7: `event: change`, one JSON object per frame, coalesced over
/// 250 ms, `: ping` comment every 30 s so a proxy does not drop an idle
/// connection.
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let raw = BroadcastStream::new(state.store.subscribe()).filter_map(|item| item.ok());
    let coalesced = tokio_stream::StreamExt::chunks_timeout(raw, 256, SSE_COALESCE);

    let stream = coalesced.filter_map(|batch: Vec<morpho_domain::ChangeEvent>| {
        if batch.is_empty() {
            return None;
        }
        // Merge the window into one frame per entity type.
        let mut merged: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for change in batch {
            let bucket = merged
                .entry(change.entity_type.as_str().to_string())
                .or_default();
            for id in change.entity_ids {
                if !bucket.contains(&id) {
                    bucket.push(id);
                }
            }
        }
        let frames: Vec<SseEvent> = merged
            .into_iter()
            .filter_map(|(entity_type, entity_ids)| {
                SseEvent::default()
                    .event("change")
                    .json_data(serde_json::json!({
                        "entity_type": entity_type,
                        "entity_ids": entity_ids,
                    }))
                    .ok()
            })
            .collect();
        // One SSE frame per poll; extra entity types in the same window follow
        // on the next one, which is still inside the coalescing budget.
        frames.into_iter().next().map(Ok)
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(SSE_PING).text(""))
}

/// `GET /api/media/{file_hash}` — serve content-addressed bytes.
pub async fn media(
    State(state): State<AppState>,
    Path(file_hash): Path<String>,
) -> ApiResult<Response> {
    if file_hash.len() != 64 || !file_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request("file_hash must be 64 hex characters"));
    }
    let lookup = file_hash.clone();
    let row = state
        .store
        .read(move |conn| queries::media_file(conn, &lookup))
        .await?;

    // rel_path comes from our own registry, but refuse to walk out of data/.
    let path = state.data_dir.join(&row.rel_path);
    if !path.starts_with(&state.data_dir) {
        return Err(ApiError::internal("media path escaped the data directory"));
    }
    let bytes = tokio::fs::read(&path).await.map_err(|err| {
        ApiError::not_found(format!(
            "media file {file_hash} is registered but unreadable: {err}"
        ))
    })?;

    let content_type = row
        .kind
        .parse::<MediaKind>()
        .map(MediaKind::content_type)
        .unwrap_or("application/octet-stream");

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            // Content-addressed: the bytes for a hash can never change.
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// OOV queue
// ---------------------------------------------------------------------------

pub async fn oov(
    State(state): State<AppState>,
    Query(query): Query<OovQuery>,
) -> ApiResult<Json<Page<OovQueueEntry>>> {
    let body = state
        .store
        .read(move |conn| queries::oov_page(conn, &query))
        .await?;
    Ok(Json(body))
}

// ---------------------------------------------------------------------------
// Dead letters
// ---------------------------------------------------------------------------

pub async fn dead_letters(
    State(state): State<AppState>,
    Query(query): Query<DeadLetterQuery>,
) -> ApiResult<Json<Page<DeadLetter>>> {
    let page = query.pagination();
    let rate_key = query.rate_key.clone();
    let body = state
        .store
        .read(move |conn| queries::dead_letters(conn, page, rate_key.as_deref()))
        .await?;
    Ok(Json(body))
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

pub async fn plan(State(state): State<AppState>) -> ApiResult<Json<PlanSummary>> {
    state
        .store
        .read(queries::plan_summary)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("no plan has been built yet"))
}

pub async fn plan_group(
    State(state): State<AppState>,
    Path(group_seq): Path<i64>,
) -> ApiResult<Json<PlanGroupDetail>> {
    state
        .store
        .read(move |conn| queries::plan_group(conn, group_seq))
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("group {group_seq} is not in the current plan")))
}

// ---------------------------------------------------------------------------
// Releases
// ---------------------------------------------------------------------------

pub async fn releases(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<Page<Release>>> {
    let page = query.pagination();
    let body = state
        .store
        .read(move |conn| queries::releases(conn, page))
        .await?;
    Ok(Json(body))
}

pub async fn release_preview(
    State(state): State<AppState>,
) -> ApiResult<Json<morpho_export::HoldbackReport>> {
    match morpho_export::preview(&state.store, &state.export).await {
        Ok(report) => Ok(Json(report)),
        Err(morpho_export::ExportError::NoPlan) => {
            Err(ApiError::not_found("no plan has been built yet"))
        }
        Err(err) => Err(ApiError::internal(err.to_string())),
    }
}

/// Subject types the resolver knows about; kept so the compiler notices if the
/// vocabulary ever grows.
const _: [SubjectType; 4] = [
    SubjectType::Word,
    SubjectType::DefCandidate,
    SubjectType::TtsInput,
    SubjectType::Global,
];
