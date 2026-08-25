//! The level-triggered reconciliation loop.
//!
//! Wake-up sources, in order of authority (README Part 4 §"对账循环"):
//!   1. startup — one full derivation, which alone guarantees convergence
//!      after any crash, deploy or offline database edit;
//!   2. timer — a full pass every 60 s; a lost change event costs a minute;
//!   3. change events — coalesced over 250 ms, then a scoped pass. Purely a
//!      latency optimization: a full pass always yields a superset.

use std::sync::Arc;
use std::time::Duration;

use morpho_domain::change::ChangeEvent;
use morpho_domain::job::JobStatus;
use morpho_domain::time::parse_ts;
use morpho_store::queries::{job_states, rate_limits, JobStateRow};
use morpho_store::{Result, Store};

use crate::dispatch::Dispatcher;
use crate::exec::{default_executors, Executor};
use crate::registry::JobRegistry;
use crate::rule::{JobSpec, Rule, Scope, Snapshot};
use crate::rules::default_rules;
use crate::text::TextPipeline;

/// Loop timing.
#[derive(Debug, Clone, Copy)]
pub struct ReconcilerConfig {
    /// Periodic full-pass interval.
    pub full_pass_interval: Duration,
    /// Window used to absorb bursts of change events.
    pub coalesce_window: Duration,
}

impl Default for ReconcilerConfig {
    fn default() -> Self {
        Self {
            full_pass_interval: Duration::from_secs(60),
            coalesce_window: Duration::from_millis(250),
        }
    }
}

/// What one pass did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PassStats {
    pub derived: usize,
    pub dispatched: usize,
    pub skipped_in_flight: usize,
    pub skipped_backoff: usize,
    pub skipped_dead: usize,
    pub skipped_waived: usize,
}

/// Why the loop woke up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Startup,
    Interval,
    Change,
}

pub struct Reconciler {
    store: Store,
    rules: Arc<Vec<Arc<dyn Rule>>>,
    dispatcher: Arc<Dispatcher>,
    registry: Arc<JobRegistry>,
    config: ReconcilerConfig,
}

impl Reconciler {
    /// Build the wave-1 reconciler: the real `ExtractTokens` rule plus stubs.
    pub fn new(store: Store, config: ReconcilerConfig) -> Self {
        let pipeline = TextPipeline::default();
        Self::with_parts(
            store,
            config,
            default_rules(pipeline.clone()),
            default_executors(pipeline),
        )
    }

    pub fn with_parts(
        store: Store,
        config: ReconcilerConfig,
        rules: Vec<Arc<dyn Rule>>,
        executors: Vec<Arc<dyn Executor>>,
    ) -> Self {
        let registry = Arc::new(JobRegistry::new());
        let dispatcher = Dispatcher::new(store.clone(), registry.clone(), executors);
        Self {
            store,
            rules: Arc::new(rules),
            dispatcher,
            registry,
            config,
        }
    }

    /// Live queue view for `GET /api/jobs`.
    pub fn registry(&self) -> Arc<JobRegistry> {
        self.registry.clone()
    }

    pub fn dispatcher(&self) -> Arc<Dispatcher> {
        self.dispatcher.clone()
    }

    /// Run until `shutdown` flips to true.
    pub async fn run(self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let mut changes = self.store.subscribe();
        let mut ticker = tokio::time::interval(self.config.full_pass_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick fires immediately; consume it so the startup pass
        // below is the one that runs first.
        ticker.tick().await;

        self.pass(Trigger::Startup, Scope::Full).await;

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                _ = ticker.tick() => {
                    self.pass(Trigger::Interval, Scope::Full).await;
                }
                received = changes.recv() => {
                    match received {
                        Ok(first) => {
                            let batch = self.coalesce(&mut changes, first).await;
                            self.pass(Trigger::Change, Scope::Partial(batch)).await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            // Dropped notifications only cost latency.
                            tracing::warn!(skipped, "change bus lagged; running a full pass");
                            self.pass(Trigger::Change, Scope::Full).await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
        tracing::info!("reconciler stopped");
    }

    /// Absorb a burst of change events into one batch.
    async fn coalesce(
        &self,
        rx: &mut tokio::sync::broadcast::Receiver<ChangeEvent>,
        first: ChangeEvent,
    ) -> Vec<ChangeEvent> {
        let mut batch = vec![first];
        let deadline = tokio::time::Instant::now() + self.config.coalesce_window;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Ok(event)) => batch.push(event),
                Ok(Err(_)) => break,
                Err(_) => break,
            }
        }
        batch
    }

    async fn pass(&self, trigger: Trigger, scope: Scope) {
        match self.run_once(scope).await {
            Ok(stats) => {
                if stats.dispatched > 0 || stats.derived > 0 {
                    tracing::debug!(?trigger, ?stats, "reconcile pass");
                }
            }
            Err(err) => tracing::error!(?trigger, error = %err, "reconcile pass failed"),
        }
    }

    /// One derivation + dispatch cycle. Public so tests can drive it directly.
    pub async fn run_once(&self, scope: Scope) -> Result<PassStats> {
        let rules = self.rules.clone();
        let now = chrono::Utc::now();

        let (mut jobs, states, limits) = self
            .store
            .read(move |conn| {
                let snapshot = Snapshot {
                    conn,
                    scope: &scope,
                    now,
                };
                let mut jobs: Vec<JobSpec> = Vec::new();
                for rule in rules.iter() {
                    let derived = rule.derive(&snapshot)?;
                    jobs.extend(derived);
                }
                Ok((jobs, job_states(conn)?, rate_limits(conn)?))
            })
            .await?;

        self.registry.configure_lanes(&limits);

        let mut stats = PassStats {
            derived: jobs.len(),
            ..PassStats::default()
        };

        let states: std::collections::HashMap<_, _> = states
            .into_iter()
            .map(|row| (row.key.clone(), row))
            .collect();

        jobs.retain_mut(|job| {
            if self.registry.is_in_flight(&job.key) {
                stats.skipped_in_flight += 1;
                return false;
            }
            match states.get(&job.key) {
                None => true,
                Some(JobStateRow {
                    status: JobStatus::Dead,
                    ..
                }) => {
                    stats.skipped_dead += 1;
                    false
                }
                Some(JobStateRow {
                    status: JobStatus::Waived,
                    ..
                }) => {
                    stats.skipped_waived += 1;
                    false
                }
                Some(row) => {
                    let due = row
                        .next_retry_at
                        .as_deref()
                        .and_then(parse_ts)
                        .is_none_or(|at| at <= now);
                    if !due {
                        stats.skipped_backoff += 1;
                        return false;
                    }
                    job.prior_attempts = Some(row.attempts);
                    true
                }
            }
        });

        jobs.sort_by(|a, b| a.ordering_key().cmp(&b.ordering_key()));
        stats.dispatched = self.dispatcher.dispatch(jobs);
        Ok(stats)
    }
}
