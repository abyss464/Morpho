use morpho_domain::types::ParseEnumError;

/// Everything that can go wrong inside the store layer.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Enum(#[from] ParseEnumError),
    /// The addressed row does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// The request is well formed but violates a business invariant.
    #[error("conflict: {0}")]
    Conflict(String),
    /// The request itself is malformed.
    #[error("invalid request: {0}")]
    Invalid(String),
    /// The request is well formed and its arguments are the right shape, but
    /// the content cannot be processed — HTTP 422 rather than 400.
    #[error("unprocessable: {0}")]
    Unprocessable(String),
    /// The writer task is gone (shutdown in progress).
    #[error("store is closed")]
    Closed,
    /// A blocking database task panicked or was cancelled.
    #[error("background database task failed: {0}")]
    Background(String),
}

impl StoreError {
    pub fn not_found(what: impl Into<String>) -> Self {
        Self::NotFound(what.into())
    }

    pub fn conflict(what: impl Into<String>) -> Self {
        Self::Conflict(what.into())
    }

    pub fn invalid(what: impl Into<String>) -> Self {
        Self::Invalid(what.into())
    }

    pub fn unprocessable(what: impl Into<String>) -> Self {
        Self::Unprocessable(what.into())
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;
