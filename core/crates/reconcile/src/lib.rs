//! The morphod reconciliation engine.
//!
//! Two paradigms, per README Part 2:
//!   * a Kubernetes-style controller — desired state is derived, never
//!     accumulated, and a full pass is always authoritative;
//!   * a build system — every derived artifact records the hash of its exact
//!     inputs, and "stale" is a comparison, never a propagated flag.
//!
//! Layout:
//!   * [`rule`] / [`rules`] — desired-state derivation for external work;
//!   * [`exec`] — one executor per job kind, each committing one atomic write;
//!   * [`stages`] — the inline local sweep (scoring, selection, OOV, liveness,
//!     distractors, plan, readiness, media GC);
//!   * [`sources`] — HTTP, WordNet, the exam corpus and the Python adapters;
//!   * [`graph`], [`score`], [`distance`], [`readiness`] — the pure algorithms.

pub mod backoff;
pub mod config;
pub mod dispatch;
pub mod distance;
pub mod engine;
pub mod exec;
pub mod facts;
pub mod graph;
pub mod lexicon;
pub mod morphy;
pub mod readiness;
pub mod registry;
pub mod rule;
pub mod rules;
pub mod score;
pub mod sources;
pub mod stages;
pub mod text;

pub use config::{AdapterConfig, ImagesConfig, SourcesConfig, ADAPTERS, ADAPTERS_DIR};
pub use dispatch::Dispatcher;
pub use engine::{EngineContext, PassStats, Reconciler, ReconcilerConfig, Trigger};
pub use exec::{default_executors, Executor};
pub use facts::Facts;
pub use graph::{build_plan, BuiltPlan, GroupType, PlanInput, PlanParams, PlanStats};
pub use lexicon::{Lexicon, LexiconCache};
pub use morphy::{
    ExceptionTable, MorphyLemmatizer, MORPHY_LEMMATIZER_VER, MORPHY_LEMMATIZER_WNDB_VER,
};
pub use readiness::{core_blockers, evaluate_all, Readiness, WordFacts};
pub use registry::{JobRegistry, Lane};
pub use rule::{JobPayload, JobSpec, Rule, ScenePrompt, Scope, Snapshot};
pub use rules::default_rules;
pub use sources::proc::{probe_adapters, AdapterProbe};
pub use sources::SourceSet;
pub use stages::SweepStats;
pub use text::{
    Lemmatizer, LowercaseLemmatizer, SimpleTokenizer, TextPipeline, Tokenizer, ABBREVIATIONS,
};
