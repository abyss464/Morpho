//! Image fetching: every enabled library in parallel, SDXL behind them all.
//!
//! README Part 4 example B states the fallback exactly: "三源尽墨 → SDXL", and
//! ruling #18 widens what "三源" means. A source counts as spent when it has
//! been queried and returned nothing, when its job is dead or waived, **or when
//! it is not enabled at all** — a stock library with no API key is permanently
//! absent, and making the word wait for eight retries against a provider that
//! will never answer helps nobody.
//!
//! What changed in wave 4 is which sources can be absent. Wikimedia Commons and
//! Openverse take no credentials, so they are never disabled, and the
//! generative fallback now fires only once two libraries that genuinely
//! answered have both come back empty — not merely because nobody supplied a
//! stock-photo key. SDXL is the last resort it was always meant to be.
//!
//! If SDXL is also absent (no ComfyUI), nothing further is derived and the word
//! reports `missing_image`, which is what the export holdback report will say.

use std::sync::Arc;

use morpho_domain::job::{JobKey, JobKind, Priority, RateKey, SubjectRef};
use morpho_domain::types::ImageSource;
use morpho_store::error::Result;

use crate::config::IMAGE_PROVIDERS;
use crate::engine::EngineContext;
use crate::facts::{image_source_name, FetchKind};
use crate::rule::{JobPayload, JobSpec, Rule, Snapshot};

pub struct FetchImagesRule {
    context: Arc<EngineContext>,
}

impl FetchImagesRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for FetchImagesRule {
    fn name(&self) -> &'static str {
        "fetch_images"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        let facts = snapshot.facts;
        let enabled = self.context.sources.config.enabled_image_sources();
        if enabled.is_empty() {
            return Ok(Vec::new());
        }

        let mut jobs = Vec::new();
        for word in &facts.active {
            if facts.words_with_images.contains(&word.word_id) {
                continue;
            }
            for provider in IMAGE_PROVIDERS {
                if !enabled.contains(&provider.source) {
                    continue;
                }
                let name = image_source_name(provider.source);
                if facts.fetched(&FetchKind::Images, word.word_id, name) {
                    continue;
                }
                jobs.push(
                    JobSpec::new(
                        JobKey::new(
                            JobKind::FetchImages,
                            SubjectRef::word_source(word.word_id, name),
                        ),
                        provider.rate_key,
                        Priority::P2,
                    )
                    .with_tiebreak(word.frequency_rank, word.word_id)
                    .with_payload(JobPayload::FetchImages {
                        word_id: word.word_id,
                        lemma: word.lemma.clone(),
                        source: provider.source,
                        gloss: facts.primary_gloss.get(&word.word_id).cloned(),
                    }),
                );
            }
        }
        Ok(jobs)
    }
}

pub struct GenImageSdxlRule {
    context: Arc<EngineContext>,
}

impl GenImageSdxlRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for GenImageSdxlRule {
    fn name(&self) -> &'static str {
        "gen_image_sdxl"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        if !self.context.sources.has_sdxl() {
            return Ok(Vec::new());
        }
        let facts = snapshot.facts;
        let enabled = self.context.sources.config.enabled_image_sources();
        let sdxl_name = image_source_name(ImageSource::Sdxl);

        let mut jobs = Vec::new();
        for word in &facts.active {
            // "Zero available candidates": a word that already has a picture
            // from anywhere never reaches the generative fallback.
            if facts.words_with_images.contains(&word.word_id) {
                continue;
            }
            if facts.fetched(&FetchKind::Images, word.word_id, sdxl_name) {
                continue;
            }
            let all_spent = IMAGE_PROVIDERS.iter().all(|provider| {
                // Not enabled => permanently absent => spent.
                !enabled.contains(&provider.source)
                    || facts.source_exhausted(
                        &FetchKind::Images,
                        JobKind::FetchImages,
                        word.word_id,
                        image_source_name(provider.source),
                    )
            });
            if !all_spent {
                continue;
            }
            jobs.push(
                JobSpec::new(
                    JobKey::new(
                        JobKind::GenImageSdxl,
                        SubjectRef::word_source(word.word_id, sdxl_name),
                    ),
                    RateKey::Sdxl,
                    // P3: the expensive generative fallback.
                    Priority::P3,
                )
                .with_tiebreak(word.frequency_rank, word.word_id)
                .with_payload(JobPayload::GenImageSdxl {
                    word_id: word.word_id,
                    lemma: word.lemma.clone(),
                    gloss: facts.primary_gloss.get(&word.word_id).cloned(),
                }),
            );
        }
        Ok(jobs)
    }
}
