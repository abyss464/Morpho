//! Desired-state rules.
//!
//! Wave 1 implements `ExtractTokens` end to end as the pattern every later
//! rule follows. The remaining kinds are registered as stubs that derive
//! nothing, so the loop, the dispatcher and `GET /api/jobs` already exercise
//! the complete shape without inventing external-source behavior.

mod extract_tokens;

use morpho_domain::job::JobKind;
use morpho_store::error::Result;

use crate::rule::{JobSpec, Rule, Snapshot};
use crate::text::TextPipeline;

pub use extract_tokens::ExtractTokensRule;

/// A registered but not-yet-implemented rule. Derives nothing.
pub struct StubRule {
    name: &'static str,
    kind: JobKind,
}

impl StubRule {
    pub fn new(kind: JobKind) -> Self {
        Self {
            name: kind.as_str(),
            kind,
        }
    }

    pub fn kind(&self) -> JobKind {
        self.kind
    }
}

impl Rule for StubRule {
    fn name(&self) -> &'static str {
        self.name
    }

    fn derive(&self, _snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        Ok(Vec::new())
    }
}

/// Job kinds whose rules are still stubs in wave 1.
pub const STUBBED_KINDS: &[JobKind] = &[
    JobKind::ScoreCandidates,
    JobKind::AutoSelect,
    JobKind::SyncOosQueue,
    JobKind::SyncAuxLiveness,
    JobKind::RecomputeReadiness,
    JobKind::BindDistractors,
    JobKind::BuildPlan,
    JobKind::GcMedia,
    JobKind::FetchDefinitions,
    JobKind::FetchExamples,
    JobKind::FetchEtymology,
    JobKind::FetchImages,
    JobKind::GenImageSdxl,
    JobKind::RewriteDefinition,
    JobKind::SynthTts,
];

/// The full wave-1 rule set, in derivation order.
pub fn default_rules(pipeline: TextPipeline) -> Vec<std::sync::Arc<dyn Rule>> {
    let mut rules: Vec<std::sync::Arc<dyn Rule>> =
        vec![std::sync::Arc::new(ExtractTokensRule::new(pipeline))];
    for kind in STUBBED_KINDS {
        rules.push(std::sync::Arc::new(StubRule::new(*kind)));
    }
    rules
}
