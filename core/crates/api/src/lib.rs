//! The morphod admin API.
//!
//! Implements `docs/contracts/admin-api.md` under `/api`, and serves the built
//! admin UI as static files when `admin-ui/dist` exists. Mutation endpoints are
//! thin translations into `WriteOp`s — the engine and the UI share one
//! transaction view, with no second channel between them.

pub mod dto;
pub mod error;
pub mod queries;
pub mod routes;
pub mod state;

use std::path::Path;

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultOnFailure, TraceLayer};
use tracing::Level;

pub use error::{ApiError, ApiResult};
pub use state::AppState;

/// Build the complete HTTP service.
///
/// `admin_ui_dist` is served as a single-page app (unknown paths fall back to
/// `index.html`) when the directory exists; otherwise `/` returns a short note
/// so a fresh checkout without a built UI is still obviously alive.
pub fn build_router(state: AppState, admin_ui_dist: Option<&Path>) -> Router {
    let api = routes::api_router(state);
    let mut router = Router::new().nest("/api", api);

    match admin_ui_dist.filter(|dir| dir.is_dir()) {
        Some(dir) => {
            tracing::info!(dir = %dir.display(), "serving admin UI");
            let index = dir.join("index.html");
            let service = ServeDir::new(dir)
                .append_index_html_on_directories(true)
                .fallback(ServeFile::new(index));
            router = router.fallback_service(service);
        }
        None => {
            router = router.fallback(placeholder);
        }
    }

    router.layer(
        TraceLayer::new_for_http()
            // A 501 from a not-yet-implemented contract endpoint is normal.
            .on_failure(DefaultOnFailure::new().level(Level::WARN)),
    )
}

async fn placeholder() -> axum::response::Response {
    use axum::response::IntoResponse;
    (
        axum::http::StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        "morphod is running. The admin UI bundle (admin-ui/dist) is not present; \
         the JSON API is available under /api.\n",
    )
        .into_response()
}
