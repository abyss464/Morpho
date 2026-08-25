//! The morphod reconciliation engine.
//!
//! Two paradigms, per README Part 2:
//!   * a Kubernetes-style controller — desired state is derived, never
//!     accumulated, and a full pass is always authoritative;
//!   * a build system — every derived artifact records the hash of its exact
//!     inputs, and "stale" is a comparison, never a propagated flag.
//!
//! Wave 1 ships the machinery plus one real rule (`ExtractTokens`) end to end.

pub mod backoff;
pub mod dispatch;
pub mod engine;
pub mod exec;
pub mod registry;
pub mod rule;
pub mod rules;
pub mod text;

pub use dispatch::Dispatcher;
pub use engine::{PassStats, Reconciler, ReconcilerConfig, Trigger};
pub use exec::{default_executors, Executor, ExtractTokensExecutor};
pub use registry::{JobRegistry, Lane};
pub use rule::{JobPayload, JobSpec, Rule, Scope, Snapshot};
pub use rules::{default_rules, ExtractTokensRule, StubRule, STUBBED_KINDS};
pub use text::{Lemmatizer, LowercaseLemmatizer, SimpleTokenizer, TextPipeline, Tokenizer};
