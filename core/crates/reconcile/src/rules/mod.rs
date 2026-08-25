//! Desired-state rules for external work.
//!
//! Each rule answers one question of the form "what does the world still owe
//! this word?", and answers it from the shared [`Facts`](crate::facts::Facts)
//! snapshot rather than by querying per word.
//!
//! The fallback chains are all expressed the same way: a fallback rule derives
//! nothing until its primary is *exhausted*, where exhausted means tried and
//! empty, dead, waived, or never configured at all. That is the single
//! mechanism behind "Wiktionary 被豁免 → Morfessor 上" and "三个图库全部标记/
//! 死信/豁免 → SDXL 上" (README Part 4 §"任务生命周期").

mod definitions;
mod etymology;
mod examples;
mod extract_tokens;
mod images;
mod tts;

use std::sync::Arc;

pub use definitions::FetchDefinitionsRule;
pub use etymology::{FetchEtymologyRule, SegmentMorphologyRule};
pub use examples::FetchExamplesRule;
pub use extract_tokens::ExtractTokensRule;
pub use images::{FetchImagesRule, GenImageSdxlRule};
pub use tts::SynthTtsRule;

use crate::engine::EngineContext;
use crate::rule::Rule;

/// The full rule set, in derivation order.
pub fn default_rules(context: Arc<EngineContext>) -> Vec<Arc<dyn Rule>> {
    vec![
        Arc::new(ExtractTokensRule::new(context.pipeline.clone())),
        Arc::new(FetchDefinitionsRule::new(context.clone())),
        Arc::new(FetchExamplesRule::new(context.clone())),
        Arc::new(FetchEtymologyRule::new(context.clone())),
        Arc::new(SegmentMorphologyRule::new(context.clone())),
        Arc::new(FetchImagesRule::new(context.clone())),
        Arc::new(GenImageSdxlRule::new(context.clone())),
        Arc::new(SynthTtsRule::new(context)),
    ]
}
