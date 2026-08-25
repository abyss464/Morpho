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
use morpho_domain::job::{JobView, JobsSnapshot, Priority};
use morpho_domain::types::MediaKind;
use morpho_store::queries::job_states;

use crate::dto::*;
use crate::error::{ApiError, ApiResult};
use crate::queries;
use crate::state::AppState;

pub async fn dashboard(State(state): State<AppState>) -> ApiResult<Json<Dashboard>> {
    let body = state.store.read(queries::dashboard).await?;
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
) -> ApiResult<Json<Page<WordRollup>>> {
    let body = state
        .store
        .read(move |conn| queries::word_list(conn, &query))
        .await?;
    Ok(Json(body))
}

pub async fn word_detail(
    State(state): State<AppState>,
    Path(word_id): Path<i64>,
) -> ApiResult<Json<WordDetail>> {
    let body = state
        .store
        .read(move |conn| queries::word_detail(conn, word_id))
        .await?;
    Ok(Json(body))
}

pub async fn jobs(State(state): State<AppState>) -> ApiResult<Json<JobsSnapshot>> {
    // In-flight work lives only in memory; `job_state` holds the persisted
    // failure rows (backoff / dead / waived).
    let backoff: Vec<JobView> = state
        .store
        .read(|conn| {
            Ok(job_states(conn)?
                .into_iter()
                .map(|row| JobView {
                    kind: row.key.kind,
                    subject_type: row.key.subject.subject_type,
                    subject_id: row.key.subject.subject_id,
                    rate_key: row.rate_key,
                    // Priority is a derivation-time property and is not
                    // persisted; report the backlog band.
                    priority: Priority::P2,
                    state: row.status.as_str().to_string(),
                    attempts: Some(row.attempts),
                    next_retry_at: row.next_retry_at,
                    last_error: row.last_error,
                })
                .collect())
        })
        .await?;
    Ok(Json(state.jobs.snapshot(backoff)))
}

/// `GET /api/stream` — server-sent `ChangeEvent`s straight off the change bus.
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let stream = BroadcastStream::new(state.store.subscribe()).filter_map(|item| match item {
        Ok(change) => Some(Ok(SseEvent::default()
            .event("change")
            .json_data(&change)
            .unwrap_or_else(|_| SseEvent::default().comment("serialization failed")))),
        // Lagged subscribers just miss a nudge; the periodic full pass covers it.
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
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
