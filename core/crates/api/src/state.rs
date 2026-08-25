//! Shared handler state.

use std::path::PathBuf;
use std::sync::Arc;

use axum::http::HeaderMap;

use morpho_domain::event::Actor;
use morpho_reconcile::JobRegistry;
use morpho_store::Store;

/// Header used to attribute an edit to a person. Wave 1 has no authentication;
/// the admin API is expected to be bound to localhost.
pub const USER_HEADER: &str = "x-morpho-user";

#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub jobs: Arc<JobRegistry>,
    /// Root of `data/`, used to resolve `media_files.rel_path`.
    pub data_dir: PathBuf,
    /// Actor name used when no `X-Morpho-User` header is present.
    pub default_user: String,
}

impl AppState {
    pub fn new(store: Store, jobs: Arc<JobRegistry>, data_dir: PathBuf) -> Self {
        Self {
            store,
            jobs,
            data_dir,
            default_user: "local".to_string(),
        }
    }

    /// Who is making this request.
    pub fn actor(&self, headers: &HeaderMap) -> Actor {
        let user = headers
            .get(USER_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&self.default_user);
        Actor::admin(user)
    }
}
