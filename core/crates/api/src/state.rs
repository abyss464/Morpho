//! Shared handler state.

use std::path::PathBuf;
use std::sync::Arc;

use axum::http::HeaderMap;

use morpho_domain::event::Actor;
use morpho_domain::tts::TtsConfig;
use morpho_export::ExportSettings;
use morpho_reconcile::JobRegistry;
use morpho_store::{MediaStore, Store};

/// Header used to attribute an edit to a person (wave-2 ruling #3).
/// There is no authentication; the admin API is expected to be bound to
/// localhost.
pub const USER_HEADER: &str = "x-morpho-user";

#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub jobs: Arc<JobRegistry>,
    /// Root of `data/`, used to resolve `media_files.rel_path`.
    pub data_dir: PathBuf,
    /// Where `POST /releases/export` writes bundles.
    pub releases_dir: PathBuf,
    /// Repository checkout root (parent of `adapters/`), for the publish
    /// pipeline that syncs assets into the Android project tree.
    pub repo_root: PathBuf,
    /// Actor name used when no `X-Morpho-User` header is present.
    pub default_user: String,
    /// Voice configuration, needed to resolve TTS content addresses.
    pub tts: TtsConfig,
    pub media: MediaStore,
    pub export: ExportSettings,
}

impl AppState {
    pub fn new(
        store: Store,
        jobs: Arc<JobRegistry>,
        data_dir: PathBuf,
        export: ExportSettings,
    ) -> Self {
        Self {
            media: MediaStore::new(&data_dir),
            releases_dir: data_dir.join("releases"),
            repo_root: PathBuf::from("."),
            tts: export.tts.clone(),
            store,
            jobs,
            data_dir,
            default_user: "local".to_string(),
            export,
        }
    }

    #[must_use]
    pub fn with_releases_dir(mut self, dir: PathBuf) -> Self {
        self.releases_dir = dir;
        self
    }

    #[must_use]
    pub fn with_repo_root(mut self, dir: PathBuf) -> Self {
        self.repo_root = dir;
        self
    }

    /// Who is making this request.
    pub fn actor(&self, headers: &HeaderMap) -> Actor {
        Actor::admin(self.user(headers))
    }

    /// The acting user's name.
    pub fn user(&self, headers: &HeaderMap) -> String {
        headers
            .get(USER_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&self.default_user)
            .to_string()
    }
}
