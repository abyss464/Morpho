//! Router assembly for `docs/contracts/admin-api.md`.
//!
//! Endpoints the contract defines but this wave does not implement answer with
//! `501` and the standard error envelope, so the admin UI sees a clear signal
//! rather than a 404 that looks like a routing bug.

pub mod read;
pub mod write;

use axum::extract::OriginalUri;
use axum::routing::{get, post};
use axum::Router;

use crate::error::ApiError;
use crate::state::AppState;

/// `nest` rewrites the inner request URI, so report the path the caller used.
async fn not_implemented(OriginalUri(uri): OriginalUri) -> ApiError {
    ApiError::not_implemented(uri.path())
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        // -- Dashboard & observability -----------------------------------
        .route("/dashboard", get(read::dashboard))
        .route("/events", get(read::events))
        .route("/jobs", get(read::jobs))
        .route("/stream", get(read::stream))
        // -- Words --------------------------------------------------------
        .route("/words", get(read::words).post(not_implemented))
        .route("/words/{id}", get(read::word_detail))
        // -- Candidates & selections --------------------------------------
        .route("/candidates/definition", post(write::mint_definition))
        .route("/candidates/example", post(not_implemented))
        .route("/candidates/image", post(not_implemented))
        .route(
            "/candidates/{kind}/{cand_id}/reject",
            post(write::reject_candidate),
        )
        .route("/selections/{kind}", post(write::set_selection))
        .route(
            "/selections/{kind}/approve",
            post(write::approve).delete(write::unapprove),
        )
        .route("/selections/definition/primary", post(not_implemented))
        .route("/selections/definition/enabled", post(not_implemented))
        // -- OOV queue -----------------------------------------------------
        .route("/oov", get(not_implemented))
        .route("/oov/{lemma}/resolve", post(write::resolve_oov))
        // -- Dead letters --------------------------------------------------
        .route("/dead-letters", get(not_implemented))
        .route("/dead-letters/retry", post(not_implemented))
        .route("/dead-letters/waive", post(not_implemented))
        // -- Plan & releases -----------------------------------------------
        .route("/plan", get(not_implemented))
        .route("/plan/groups/{seq}", get(not_implemented))
        .route("/releases", get(not_implemented))
        .route("/releases/preview", get(not_implemented))
        .route("/releases/export", post(not_implemented))
        // -- Media ----------------------------------------------------------
        .route("/media/{file_hash}", get(read::media))
        .with_state(state)
}
