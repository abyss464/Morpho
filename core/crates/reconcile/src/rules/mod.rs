//! Desired-state rules for external work.
//!
//! Each rule answers one question of the form "what does the world still owe
//! this word?", and answers it from the shared [`Facts`](crate::facts::Facts)
//! snapshot rather than by querying per word.
//!
//! The fallback chains are all expressed the same way: a fallback rule derives
//! nothing until its primary is *exhausted*, where exhausted means tried and
//! empty, dead, waived, or never configured at all. That is the single
//! mechanism behind "Wiktionary 被豁免 → Morfessor 上" and "图库全部标记/
//! 死信/豁免 → SDXL 上" (README Part 4 §"任务生命周期").
//!
//! Ruling #18 changes which sources can be missing rather than how the chains
//! work. Wikimedia Commons, Openverse and Tatoeba take no credentials, so they
//! are never "never configured" — which means the generative image fallback now
//! waits for libraries that genuinely answered, and a checkout with no accounts
//! still reaches real content instead of the end of every chain.
//!
//! The image chain has one more link than the others: a provider that answered
//! and found nothing is asked again on looser terms before anything is
//! generated. Those retries are staged exactly like a fallback — each one waits
//! for the mark of the one before it — so a word costs one extra request per
//! pass rather than a burst of them (see [`images`]).

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
pub use images::{FetchImagesRule, FetchImagesSecondPassRule, GenImageSdxlRule, GenSceneImageRule};
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
        Arc::new(FetchImagesSecondPassRule::new(context.clone())),
        Arc::new(GenImageSdxlRule::new(context.clone())),
        Arc::new(GenSceneImageRule::new(context.clone())),
        Arc::new(SynthTtsRule::new(context)),
    ]
}
