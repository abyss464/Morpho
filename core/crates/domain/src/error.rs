//! The error taxonomy that drives retry behavior (README Part 4, adapter
//! protocol §Envelope):
//!
//! * `Permanent` — a legitimate empty/impossible result. Record a completion
//!   marker, never retry.
//! * `Transient` — network / 5xx / timeout. Exponential backoff.
//! * `RateLimited` — park the whole lane until the given instant; the attempt
//!   does not count against the dead-letter threshold.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Wire form of the taxonomy, as spoken by `adapters/*` on stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Permanent,
    Transient,
    RateLimited,
}

/// Failure of a single reconciler job.
#[derive(Debug, Clone, thiserror::Error)]
pub enum TaskError {
    #[error("permanent: {0}")]
    Permanent(String),
    #[error("transient: {0}")]
    Transient(String),
    #[error("rate limited for {}ms", .retry_after.as_millis())]
    RateLimited {
        until: Instant,
        retry_after: Duration,
    },
}

impl TaskError {
    pub fn permanent(msg: impl Into<String>) -> Self {
        Self::Permanent(msg.into())
    }

    pub fn transient(msg: impl Into<String>) -> Self {
        Self::Transient(msg.into())
    }

    pub fn rate_limited(retry_after: Duration) -> Self {
        Self::RateLimited {
            until: Instant::now() + retry_after,
            retry_after,
        }
    }

    pub fn rate_limited_ms(retry_after_ms: u64) -> Self {
        Self::rate_limited(Duration::from_millis(retry_after_ms))
    }

    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Permanent(_) => ErrorKind::Permanent,
            Self::Transient(_) => ErrorKind::Transient,
            Self::RateLimited { .. } => ErrorKind::RateLimited,
        }
    }

    /// Whether this failure should increment `job_state.attempts`.
    /// Rate limiting is a property of the lane, not of the subject.
    pub fn counts_as_attempt(&self) -> bool {
        !matches!(self, Self::RateLimited { .. })
    }

    /// Whether the reconciler should ever try this subject again.
    pub fn is_retryable(&self) -> bool {
        !matches!(self, Self::Permanent(_))
    }

    /// Message for `job_state.last_error`.
    pub fn message(&self) -> String {
        match self {
            Self::Permanent(m) | Self::Transient(m) => m.clone(),
            Self::RateLimited { retry_after, .. } => {
                format!("rate limited, retry after {}ms", retry_after.as_millis())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_flags() {
        let p = TaskError::permanent("404");
        assert!(!p.is_retryable());
        assert!(p.counts_as_attempt());
        assert_eq!(p.kind(), ErrorKind::Permanent);

        let t = TaskError::transient("connection reset");
        assert!(t.is_retryable());
        assert!(t.counts_as_attempt());

        let r = TaskError::rate_limited_ms(1_500);
        assert!(r.is_retryable());
        assert!(!r.counts_as_attempt());
        assert_eq!(r.kind(), ErrorKind::RateLimited);
        assert!(r.message().contains("1500"));
    }

    #[test]
    fn wire_kind_spelling() {
        assert_eq!(
            serde_json::to_string(&ErrorKind::RateLimited).unwrap(),
            "\"rate_limited\""
        );
    }
}
