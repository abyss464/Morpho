//! In-flight job registry and dispatcher lanes.
//!
//! `QUEUED`/`RUNNING` exist only here, in memory. Losing them to a crash costs
//! nothing: the startup full pass rediscovers every unmet need
//! (README Part 4 §"任务生命周期").

use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

use morpho_domain::job::{JobKey, JobView, JobsSnapshot, LaneView, Priority, RateKey};
use morpho_domain::time::format_ts;
use morpho_store::queries::RateLimitRow;

use crate::rule::JobSpec;

type DirectLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Fallback limits for a lane with no `rate_limits` row.
pub const FALLBACK_LIMIT: RateLimitRow = RateLimitRow {
    rate_key: RateKey::Cpu,
    max_concurrency: 2,
    refill_per_min: 60.0,
    burst: 5,
};

/// One dispatcher swim lane: a token bucket plus a concurrency semaphore.
pub struct Lane {
    pub rate_key: RateKey,
    pub limit: usize,
    pub semaphore: Arc<tokio::sync::Semaphore>,
    limiter: DirectLimiter,
    queued: AtomicUsize,
    running: AtomicUsize,
    parked_until: Mutex<Option<Instant>>,
}

impl Lane {
    fn new(row: &RateLimitRow, rate_key: RateKey) -> Self {
        let limit = row.max_concurrency.clamp(1, 4096) as usize;
        let refill = if row.refill_per_min > 0.0 {
            row.refill_per_min
        } else {
            FALLBACK_LIMIT.refill_per_min
        };
        let period = Duration::from_secs_f64((60.0 / refill).max(0.000_001));
        let burst = NonZeroU32::new(row.burst.clamp(1, 100_000) as u32).expect("burst >= 1");
        let quota = Quota::with_period(period)
            .expect("non-zero refill period")
            .allow_burst(burst);
        Self {
            rate_key,
            limit,
            semaphore: Arc::new(tokio::sync::Semaphore::new(limit)),
            limiter: RateLimiter::direct(quota),
            queued: AtomicUsize::new(0),
            running: AtomicUsize::new(0),
            parked_until: Mutex::new(None),
        }
    }

    /// Wait for a token from this lane's bucket.
    pub async fn wait_for_token(&self) {
        self.limiter.until_ready().await;
    }

    /// If the lane is parked (a `RateLimited` response), how long is left.
    pub fn park_remaining(&self) -> Option<Duration> {
        let guard = self.parked_until.lock().expect("lane mutex poisoned");
        guard.and_then(|until| until.checked_duration_since(Instant::now()))
    }

    /// Park the whole lane until `until` (never shortens an existing park).
    pub fn park_until(&self, until: Instant) {
        let mut guard = self.parked_until.lock().expect("lane mutex poisoned");
        if guard.is_none_or(|current| until > current) {
            *guard = Some(until);
        }
    }

    pub fn enter_queue(&self) {
        self.queued.fetch_add(1, Ordering::Relaxed);
    }

    pub fn start_running(&self) {
        self.queued.fetch_sub(1, Ordering::Relaxed);
        self.running.fetch_add(1, Ordering::Relaxed);
    }

    pub fn finish_running(&self) {
        self.running.fetch_sub(1, Ordering::Relaxed);
    }

    fn view(&self) -> LaneView {
        LaneView {
            queued: self.queued.load(Ordering::Relaxed),
            running: self.running.load(Ordering::Relaxed),
            limit: self.limit,
            parked_until: self.park_remaining().map(|left| {
                format_ts(chrono::Utc::now() + chrono::Duration::from_std(left).unwrap_or_default())
            }),
        }
    }
}

#[derive(Debug, Clone)]
struct InFlightEntry {
    rate_key: RateKey,
    priority: Priority,
    running: bool,
}

/// Deduplicates work and exposes the live queue to `GET /api/jobs`.
pub struct JobRegistry {
    in_flight: Mutex<HashMap<JobKey, InFlightEntry>>,
    lanes: Mutex<BTreeMap<RateKey, Arc<Lane>>>,
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            in_flight: Mutex::new(HashMap::new()),
            lanes: Mutex::new(BTreeMap::new()),
        }
    }

    /// Create any lane described by `rate_limits` that does not exist yet.
    ///
    /// Existing lanes are left alone: rebuilding a live semaphore underneath
    /// running tasks is not worth the complexity, so limit changes take effect
    /// on restart.
    pub fn configure_lanes(&self, limits: &[RateLimitRow]) {
        let mut lanes = self.lanes.lock().expect("lane map poisoned");
        for row in limits {
            lanes
                .entry(row.rate_key)
                .or_insert_with(|| Arc::new(Lane::new(row, row.rate_key)));
        }
    }

    /// Get (or lazily create) the lane for a rate key.
    pub fn lane(&self, rate_key: RateKey) -> Arc<Lane> {
        let mut lanes = self.lanes.lock().expect("lane map poisoned");
        lanes
            .entry(rate_key)
            .or_insert_with(|| {
                tracing::warn!(%rate_key, "no rate_limits row; using fallback lane limits");
                Arc::new(Lane::new(&FALLBACK_LIMIT, rate_key))
            })
            .clone()
    }

    /// True if this exact job is already queued or running.
    pub fn is_in_flight(&self, key: &JobKey) -> bool {
        self.in_flight
            .lock()
            .expect("in-flight map poisoned")
            .contains_key(key)
    }

    /// Claim a job. Returns false if an identical job is already in flight.
    pub fn try_claim(&self, spec: &JobSpec) -> bool {
        let mut map = self.in_flight.lock().expect("in-flight map poisoned");
        if map.contains_key(&spec.key) {
            return false;
        }
        map.insert(
            spec.key.clone(),
            InFlightEntry {
                rate_key: spec.rate_key,
                priority: spec.priority,
                running: false,
            },
        );
        true
    }

    pub fn mark_running(&self, key: &JobKey) {
        if let Some(entry) = self
            .in_flight
            .lock()
            .expect("in-flight map poisoned")
            .get_mut(key)
        {
            entry.running = true;
        }
    }

    pub fn release(&self, key: &JobKey) {
        self.in_flight
            .lock()
            .expect("in-flight map poisoned")
            .remove(key);
    }

    pub fn in_flight_len(&self) -> usize {
        self.in_flight.lock().expect("in-flight map poisoned").len()
    }

    /// Body of `GET /api/jobs`. `backoff` comes from `job_state`, which is the
    /// only part of the queue that is persisted.
    pub fn snapshot(&self, backoff: Vec<JobView>) -> JobsSnapshot {
        let mut in_flight: Vec<JobView> = self
            .in_flight
            .lock()
            .expect("in-flight map poisoned")
            .iter()
            .map(|(key, entry)| JobView {
                kind: key.kind,
                subject_type: key.subject.subject_type,
                subject_id: key.subject.subject_id.clone(),
                rate_key: entry.rate_key,
                priority: entry.priority,
                state: if entry.running { "running" } else { "queued" }.to_string(),
                attempts: None,
                next_retry_at: None,
                last_error: None,
            })
            .collect();
        in_flight.sort_by(|a, b| {
            (a.priority, a.kind, &a.subject_id).cmp(&(b.priority, b.kind, &b.subject_id))
        });

        let lanes = self
            .lanes
            .lock()
            .expect("lane map poisoned")
            .iter()
            .map(|(key, lane)| (key.as_str().to_string(), lane.view()))
            .collect();

        JobsSnapshot {
            in_flight,
            backoff,
            lanes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morpho_domain::job::{JobKind, SubjectRef};

    fn spec(id: i64) -> JobSpec {
        JobSpec::new(
            JobKey::new(JobKind::ExtractTokens, SubjectRef::def_candidate(id)),
            RateKey::Cpu,
            Priority::P0,
        )
    }

    #[test]
    fn claims_are_exclusive_until_released() {
        let registry = JobRegistry::new();
        let job = spec(1);
        assert!(registry.try_claim(&job));
        assert!(!registry.try_claim(&job));
        assert!(registry.is_in_flight(&job.key));
        assert_eq!(registry.in_flight_len(), 1);
        registry.release(&job.key);
        assert!(registry.try_claim(&job));
    }

    #[test]
    fn snapshot_reports_lane_counters() {
        let registry = JobRegistry::new();
        registry.configure_lanes(&[RateLimitRow {
            rate_key: RateKey::Cpu,
            max_concurrency: 4,
            refill_per_min: 600.0,
            burst: 16,
        }]);
        let job = spec(7);
        registry.try_claim(&job);
        let lane = registry.lane(RateKey::Cpu);
        lane.enter_queue();
        lane.start_running();
        registry.mark_running(&job.key);

        let snapshot = registry.snapshot(Vec::new());
        assert_eq!(snapshot.in_flight.len(), 1);
        assert_eq!(snapshot.in_flight[0].state, "running");
        let cpu = &snapshot.lanes["cpu"];
        assert_eq!(cpu.limit, 4);
        assert_eq!(cpu.running, 1);
        assert_eq!(cpu.queued, 0);
    }

    #[test]
    fn parking_never_shortens() {
        let lane = Lane::new(&FALLBACK_LIMIT, RateKey::Freedict);
        assert!(lane.park_remaining().is_none());
        lane.park_until(Instant::now() + Duration::from_secs(60));
        lane.park_until(Instant::now() + Duration::from_secs(5));
        let left = lane.park_remaining().expect("parked");
        assert!(left > Duration::from_secs(30));
    }
}
