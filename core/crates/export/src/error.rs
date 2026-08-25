//! Export failures.

use std::path::PathBuf;

/// Everything that can stop an export.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("store error: {0}")]
    Store(#[from] morpho_store::StoreError),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// No plan has been built yet, so there is nothing to order a release by.
    #[error("no current plan; the reconciler has not built one yet")]
    NoPlan,
    #[error("output directory already exists: {0}")]
    OutputExists(PathBuf),
    #[error("media file {0} is referenced but not registered")]
    MissingMedia(String),
    #[error("cannot read media file {0}: {1}")]
    UnreadableMedia(String, std::io::Error),
    #[error("media file content address drifted: expected {expected}, found {actual}")]
    MediaHashMismatch { expected: String, actual: String },
    #[error("word {0} passed the gates without a {1}")]
    MissingAsset(i64, &'static str),
    /// The hard validation gates rejected the cut (README Part 5).
    #[error("{} export validation gate(s) failed", .0.len())]
    GatesFailed(Vec<crate::GateFailure>),
}

pub type ExportResult<T> = std::result::Result<T, ExportError>;
