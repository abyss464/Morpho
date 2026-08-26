//! The inline maintenance sweep.
//!
//! README Part 4 puts scoring, automatic selection, graph work, distractors and
//! readiness on the local `cpu` lane. They run here instead of as dispatched
//! jobs, in a fixed order, once per reconcile pass, because each one reads what
//! the previous one wrote: scoring feeds selection, selection changes the
//! dependency edges, the edges change the plan, and the plan is an input to
//! readiness. Dispatching them separately would spread one convergence over as
//! many passes as there are stages, for no gain — they are milliseconds of work
//! at 6 000 words.
//!
//! Every stage is idempotent and computes from a fresh read snapshot, so a
//! crash between two of them costs nothing: the next pass runs the whole
//! sequence again.

mod aux_liveness;
mod distractors;
mod media_gc;
mod oos;
mod plan;
mod readiness;
mod select;

use std::time::{Duration, Instant};

use morpho_store::error::Result;
use morpho_store::Store;

use crate::engine::EngineContext;

pub use distractors::bind_distractors;
pub use media_gc::{sweep_media, MEDIA_GC_GRACE};
pub use plan::{build_plan, plan_input_hash};
pub use readiness::recompute_readiness;
pub use select::{auto_select, score_candidates};

/// What one sweep changed. Purely for logging and tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepStats {
    pub scored: usize,
    pub selected: usize,
    pub oos_opened: usize,
    pub oos_closed: usize,
    pub aux_retired: usize,
    pub aux_reactivated: usize,
    pub distractors_bound: usize,
    pub plan_rebuilt: bool,
    pub readiness_changed: usize,
    pub media_marked: usize,
}

impl SweepStats {
    pub fn is_quiet(&self) -> bool {
        *self == Self::default()
    }
}

/// Media GC is expensive relative to how fast anything can become garbage, so
/// it runs on its own slower cadence.
pub const MEDIA_GC_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Plan rebuild debounce (README Part 3: 2 s to absorb an edit storm).
pub const PLAN_DEBOUNCE: Duration = Duration::from_secs(2);

/// Run the whole sweep in order.
pub async fn run(
    store: &Store,
    context: &EngineContext,
    clocks: &SweepClocks,
) -> Result<SweepStats> {
    // Before anything reads a lemma: the lemmatizer validates candidates
    // against this set, and every stage below plus the `ExtractTokens` rule
    // derived after the sweep must see the same one.
    refresh_lexicon(store, context).await?;

    let scored = score_candidates(store, context).await?;
    let selected = auto_select(store, context).await?;
    let (oos_opened, oos_closed) = oos::sync(store).await?;
    let (aux_retired, aux_reactivated) = aux_liveness::sync(store).await?;
    let distractors_bound = bind_distractors(store).await?;

    let mut stats = SweepStats {
        scored,
        selected,
        oos_opened,
        oos_closed,
        aux_retired,
        aux_reactivated,
        distractors_bound,
        ..SweepStats::default()
    };

    if clocks.plan_due() {
        stats.plan_rebuilt = build_plan(store, context).await?;
        clocks.mark_plan_built();
    }

    stats.readiness_changed = recompute_readiness(store, context).await?;

    if clocks.media_gc_due() {
        stats.media_marked = sweep_media(store).await?;
        clocks.mark_media_gc();
    }

    Ok(stats)
}

/// Re-read `words.lemma` into the pipeline's lexicon cache.
///
/// Not a stage: it writes nothing and changes no desired state. It is the
/// pass's first act because the whole point of a level-triggered loop is that
/// every derivation in a pass sees one consistent world, and a lemma the
/// lemmatizer may point a token at is part of that world.
pub async fn refresh_lexicon(store: &Store, context: &EngineContext) -> Result<usize> {
    let cache = context.pipeline.lexicon();
    let count = store.read(move |conn| cache.refresh(conn)).await?;
    tracing::trace!(lemmas = count, "lexicon snapshot refreshed");
    Ok(count)
}

/// Debounce state for the stages that do not run every pass.
#[derive(Debug)]
pub struct SweepClocks {
    last_plan: std::sync::Mutex<Option<Instant>>,
    last_media_gc: std::sync::Mutex<Option<Instant>>,
}

impl Default for SweepClocks {
    fn default() -> Self {
        Self::new()
    }
}

impl SweepClocks {
    pub fn new() -> Self {
        Self {
            last_plan: std::sync::Mutex::new(None),
            last_media_gc: std::sync::Mutex::new(None),
        }
    }

    fn plan_due(&self) -> bool {
        let guard = self.last_plan.lock().expect("plan clock poisoned");
        guard.is_none_or(|at| at.elapsed() >= PLAN_DEBOUNCE)
    }

    fn mark_plan_built(&self) {
        *self.last_plan.lock().expect("plan clock poisoned") = Some(Instant::now());
    }

    fn media_gc_due(&self) -> bool {
        let guard = self.last_media_gc.lock().expect("gc clock poisoned");
        guard.is_none_or(|at| at.elapsed() >= MEDIA_GC_INTERVAL)
    }

    fn mark_media_gc(&self) {
        *self.last_media_gc.lock().expect("gc clock poisoned") = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_clocks_fire_on_the_first_pass() {
        let clocks = SweepClocks::new();
        assert!(clocks.plan_due());
        assert!(clocks.media_gc_due());
    }

    #[test]
    fn the_plan_debounces_after_a_build() {
        let clocks = SweepClocks::new();
        clocks.mark_plan_built();
        assert!(
            !clocks.plan_due(),
            "an edit storm must not rebuild per edit"
        );
    }

    #[test]
    fn media_gc_waits_much_longer_than_the_plan() {
        assert!(MEDIA_GC_INTERVAL > PLAN_DEBOUNCE * 100);
        let clocks = SweepClocks::new();
        clocks.mark_media_gc();
        assert!(!clocks.media_gc_due());
    }

    #[test]
    fn a_quiet_sweep_is_detectable() {
        assert!(SweepStats::default().is_quiet());
        assert!(!SweepStats {
            scored: 1,
            ..SweepStats::default()
        }
        .is_quiet());
    }
}
