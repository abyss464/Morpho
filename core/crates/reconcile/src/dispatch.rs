//! The dispatcher: one swim lane per rate key, each with a token bucket and a
//! concurrency semaphore, plus the failure-classification policy.

use std::collections::HashMap;
use std::sync::Arc;

use morpho_domain::error::TaskError;
use morpho_domain::event::Actor;
use morpho_domain::job::{JobKind, JobStatus};
use morpho_domain::time::format_ts;
use morpho_store::ops::UpsertJobState;
use morpho_store::{Store, WriteOp};

use crate::backoff::retry_delay;
use crate::exec::Executor;
use crate::registry::JobRegistry;
use crate::rule::JobSpec;

pub struct Dispatcher {
    store: Store,
    registry: Arc<JobRegistry>,
    executors: HashMap<JobKind, Arc<dyn Executor>>,
}

impl Dispatcher {
    pub fn new(
        store: Store,
        registry: Arc<JobRegistry>,
        executors: Vec<Arc<dyn Executor>>,
    ) -> Arc<Self> {
        let executors = executors
            .into_iter()
            .map(|executor| (executor.kind(), executor))
            .collect();
        Arc::new(Self {
            store,
            registry,
            executors,
        })
    }

    pub fn handles(&self, kind: JobKind) -> bool {
        self.executors.contains_key(&kind)
    }

    /// Claim and spawn every job that is not already in flight.
    ///
    /// Jobs whose kind has no executor yet are dropped with a debug log: the
    /// rule that produced them is a stub, and the next pass will derive them
    /// again for free.
    pub fn dispatch(self: &Arc<Self>, jobs: Vec<JobSpec>) -> usize {
        let mut spawned = 0;
        for job in jobs {
            let Some(executor) = self.executors.get(&job.key.kind).cloned() else {
                tracing::debug!(kind = %job.key.kind, "no executor registered; skipping job");
                continue;
            };
            if !self.registry.try_claim(&job) {
                continue;
            }
            let this = Arc::clone(self);
            tokio::spawn(async move {
                this.run_job(executor, job).await;
            });
            spawned += 1;
        }
        spawned
    }

    async fn run_job(&self, executor: Arc<dyn Executor>, job: JobSpec) {
        let lane = self.registry.lane(job.rate_key);
        lane.enter_queue();

        // A lane parked by a RateLimited response stalls every job on it.
        if let Some(remaining) = lane.park_remaining() {
            tokio::time::sleep(remaining).await;
        }

        let permit = lane.semaphore.clone().acquire_owned().await;
        lane.wait_for_token().await;
        lane.start_running();
        self.registry.mark_running(&job.key);

        let result = executor.run(&job, &self.store).await;

        lane.finish_running();
        drop(permit);

        if let Err(err) = self.record_outcome(&job, result).await {
            tracing::error!(job = %job.key, error = %err, "failed to persist job outcome");
        }
        self.registry.release(&job.key);
    }

    async fn record_outcome(
        &self,
        job: &JobSpec,
        result: Result<(), TaskError>,
    ) -> morpho_store::Result<()> {
        let actor = Actor::Worker(job.key.kind);
        match result {
            Ok(()) => {
                // No row means healthy; only pay for the delete if a row existed.
                if job.prior_attempts.is_some() {
                    self.store
                        .write(
                            actor,
                            WriteOp::ClearJobState {
                                key: job.key.clone(),
                            },
                        )
                        .await?;
                }
                Ok(())
            }
            Err(TaskError::RateLimited { until, .. }) => {
                // Park the lane; the attempt does not count against the
                // dead-letter threshold, and the job is re-derived next pass.
                self.registry.lane(job.rate_key).park_until(until);
                tracing::warn!(lane = %job.rate_key, job = %job.key, "lane parked by rate limit");
                Ok(())
            }
            Err(err) => {
                let attempts = job.prior_attempts.unwrap_or(0) + 1;
                let permanent = !err.is_retryable();
                let exhausted = attempts >= i64::from(job.key.kind.dead_after_attempts());
                let (status, next_retry_at) = if permanent || exhausted {
                    (JobStatus::Dead, None)
                } else {
                    let delay = retry_delay(&job.key, attempts);
                    let at = chrono::Utc::now()
                        + chrono::Duration::from_std(delay).unwrap_or_else(|_| {
                            chrono::Duration::seconds(crate::backoff::MAX_DELAY.as_secs() as i64)
                        });
                    (JobStatus::Backoff, Some(format_ts(at)))
                };
                if status == JobStatus::Dead {
                    tracing::warn!(job = %job.key, attempts, error = %err, "job dead-lettered");
                }
                self.store
                    .write(
                        actor,
                        WriteOp::UpsertJobState(UpsertJobState {
                            key: job.key.clone(),
                            rate_key: job.rate_key,
                            status,
                            attempts,
                            next_retry_at,
                            last_error: Some(err.message()),
                        }),
                    )
                    .await?;
                Ok(())
            }
        }
    }

    /// Wait until nothing is in flight. Test and shutdown helper.
    pub async fn drain(&self) {
        while self.registry.in_flight_len() > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
}
