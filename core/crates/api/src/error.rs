//! Error envelope.
//!
//! `docs/contracts/admin-api.md`: every failure is
//! `{"error": {"code": "string", "message": "string"}}` with a matching HTTP
//! status.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use morpho_store::StoreError;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }

    /// Endpoint declared by the contract but not implemented in this wave.
    pub fn not_implemented(path: &str) -> Self {
        Self::new(
            StatusCode::NOT_IMPLEMENTED,
            "not_implemented",
            format!("{path} is not implemented yet in this build of morphod"),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // 501 is an expected answer for a contract endpoint this build does not
        // implement yet; only genuine faults deserve an error-level log.
        if self.status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(code = self.code, message = %self.message, "api error");
        } else {
            tracing::debug!(status = %self.status, code = self.code, message = %self.message, "api error");
        }
        (
            self.status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code: self.code.to_string(),
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

impl From<StoreError> for ApiError {
    fn from(err: StoreError) -> Self {
        match err {
            StoreError::NotFound(what) => ApiError::not_found(what),
            StoreError::Conflict(what) => ApiError::conflict(what),
            StoreError::Invalid(what) => ApiError::bad_request(what),
            StoreError::Closed => ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "the store is shutting down",
            ),
            other => ApiError::internal(other.to_string()),
        }
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(err: rusqlite::Error) -> Self {
        ApiError::internal(err.to_string())
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;
