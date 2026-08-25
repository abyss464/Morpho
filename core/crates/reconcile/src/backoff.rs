//! Retry backoff.
//!
//! README Part 4: `next_retry_at = now + min(30s · 2^attempts, 1h) ± 20% jitter`.
//!
//! The jitter is derived from the job key rather than from a random number
//! generator: the point of jitter is to decorrelate *different* subjects that
//! failed together, and hashing the key achieves that while keeping retries
//! reproducible in tests and in incident forensics.

use std::time::Duration;

use morpho_domain::job::JobKey;

pub const BASE_DELAY: Duration = Duration::from_secs(30);
pub const MAX_DELAY: Duration = Duration::from_secs(3_600);
const JITTER_FRACTION: f64 = 0.2;

/// Delay before the next attempt of `key`, after `attempts` failures.
pub fn retry_delay(key: &JobKey, attempts: i64) -> Duration {
    let exponent = attempts.clamp(0, 16) as u32;
    let scaled = BASE_DELAY
        .checked_mul(1u32 << exponent.min(20))
        .unwrap_or(MAX_DELAY)
        .min(MAX_DELAY);
    let factor = 1.0 + JITTER_FRACTION * jitter_unit(key, attempts);
    Duration::from_secs_f64(scaled.as_secs_f64() * factor)
}

/// Deterministic value in `[-1.0, 1.0)` derived from the key and attempt.
fn jitter_unit(key: &JobKey, attempts: i64) -> f64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(key.to_string().as_bytes());
    hasher.update(&attempts.to_le_bytes());
    let bytes = hasher.finalize();
    let raw = u32::from_le_bytes(bytes.as_bytes()[..4].try_into().expect("4 bytes"));
    (f64::from(raw) / f64::from(u32::MAX)) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use morpho_domain::job::{JobKind, SubjectRef};

    fn key(id: i64) -> JobKey {
        JobKey::new(JobKind::FetchDefinitions, SubjectRef::word(id))
    }

    #[test]
    fn grows_exponentially_then_caps() {
        let k = key(1);
        let d1 = retry_delay(&k, 1).as_secs_f64();
        let d2 = retry_delay(&k, 2).as_secs_f64();
        let d3 = retry_delay(&k, 3).as_secs_f64();
        assert!((48.0..=72.0).contains(&d1), "{d1}");
        assert!((96.0..=144.0).contains(&d2), "{d2}");
        assert!(d3 > d2 && d2 > d1);
        for attempts in 8..20 {
            let capped = retry_delay(&k, attempts).as_secs_f64();
            assert!(capped <= MAX_DELAY.as_secs_f64() * 1.2 + 1.0, "{capped}");
        }
    }

    #[test]
    fn stays_within_twenty_percent_of_the_nominal_delay() {
        for id in 0..200 {
            let d = retry_delay(&key(id), 2).as_secs_f64();
            assert!((96.0..=144.0).contains(&d), "id {id}: {d}");
        }
    }

    #[test]
    fn is_deterministic_but_decorrelated_across_subjects() {
        assert_eq!(retry_delay(&key(1), 3), retry_delay(&key(1), 3));
        let a = retry_delay(&key(1), 3);
        let b = retry_delay(&key(2), 3);
        assert_ne!(a, b);
    }
}
