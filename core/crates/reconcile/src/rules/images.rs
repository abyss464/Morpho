//! Image fetching: every enabled library in parallel, second passes behind
//! them, SDXL behind everything.
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
//! stock-photo key.
//!
//! What changes here is that "the libraries came back empty" stopped meaning
//! "the libraries have nothing". A live pass left several hundred words with no
//! candidate at all, and for most of them the picture exists — it is filed
//! under a licence the strict filter excludes, or under words the query never
//! used. So a word that runs out walks a chain of **second passes**, one
//! request at a time, each asking a provider that already answered to answer
//! again on looser terms:
//!
//! ```text
//! strict passes → openverse (relaxed licence) → wikimedia (widened query)
//!               → openverse (widened query)   → sdxl, if configured
//! ```
//!
//! Every stage owns a completion mark of its own, so the chain is resumable
//! and, more to the point, so a database whose words were all searched months
//! ago re-enters it without anything being cleared. Each stage waits for the
//! ones before it: a word gets one extra request per pass, not four at once,
//! and the cheapest widening is tried first.
//!
//! Generation is last on purpose. A picture of something that exists beats a
//! picture of something that does not, however loosely licensed, so SDXL now
//! waits for the second passes as well as the first ones. If SDXL is also
//! absent (no ComfyUI), nothing further is derived and the word reports
//! `missing_image`, which is what the export holdback report will say.
//!
//! Wave 8 widened what "ran out" means a second time. Media is content
//! addressed, so `adapt`, `adapter` and `adaptation` come back from every
//! provider holding one byte-identical stock photo between them; whichever
//! selects first takes it, and the others hold a pool the selector will never
//! ship, because a question renders the word beside its three fixed distractors
//! and two identical option images make the card unanswerable. Such a word has
//! candidates and is no better served than one nobody answered for, so
//! [`Facts::needs_image_candidates`] treats it as having none and it enters the
//! chain exactly like a word with an empty pool.

use std::sync::Arc;

use morpho_domain::job::{JobKey, JobKind, JobStatus, Priority, RateKey, SubjectRef};
use morpho_domain::types::ImageSource;
use morpho_store::error::Result;

use crate::config::{IMAGE_PROVIDERS, IMAGE_SECOND_PASSES};
use crate::engine::EngineContext;
use crate::facts::{image_source_name, Facts, FetchKind};
use crate::rule::{JobPayload, JobSpec, Rule, Snapshot};
use crate::score::ImageStrategy;

/// Has this image pass finished, one way or another?
///
/// A completion mark is the primary signal and the only one a successful pass
/// leaves — a zero-result fetch writes one, which is the whole reason the mark
/// exists (README Part 4 §"完成标记"). A dead or waived job is the other way a
/// pass ends: the word will never get an answer out of it, and the chain must
/// not wait forever for one.
///
/// Deliberately weaker than [`Facts::source_exhausted`], which additionally
/// insists the pass produced nothing. The stage gate here is paired with a
/// separate "the word still has no picture" check, so a pass that returned
/// results which never became a usable candidate should let the chain continue
/// rather than strand the word.
fn pass_spent(facts: &Facts, word_id: i64, mark: &str) -> bool {
    if facts.fetched(&FetchKind::Images, word_id, mark) {
        return true;
    }
    matches!(
        facts.job_status(&JobKey::new(
            JobKind::FetchImages,
            SubjectRef::word_source(word_id, mark)
        )),
        Some(JobStatus::Dead | JobStatus::Waived)
    )
}

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
            if !facts.needs_image_candidates(word.word_id) {
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
                        strategy: ImageStrategy::Strict,
                        mark: name.to_string(),
                        gloss_tokens: Vec::new(),
                    }),
                );
            }
        }
        Ok(jobs)
    }
}

/// The second passes over the keyless providers, one stage at a time.
pub struct FetchImagesSecondPassRule {
    context: Arc<EngineContext>,
}

impl FetchImagesSecondPassRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for FetchImagesSecondPassRule {
    fn name(&self) -> &'static str {
        "fetch_images_second_pass"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        let facts = snapshot.facts;
        let enabled = self.context.sources.config.enabled_image_sources();

        let mut jobs = Vec::new();
        for word in &facts.active {
            // The trigger is the same one the generative fallback reads: zero
            // *usable* candidates, from anywhere.
            if !facts.needs_image_candidates(word.word_id) {
                continue;
            }
            // Nothing is retried before everything has been tried once. A word
            // still waiting on a first pass may yet be answered by it, and a
            // strict hit is worth more than any of these.
            let strict_done = IMAGE_PROVIDERS.iter().all(|provider| {
                !enabled.contains(&provider.source)
                    || pass_spent(facts, word.word_id, image_source_name(provider.source))
            });
            if !strict_done {
                continue;
            }

            for pass in IMAGE_SECOND_PASSES {
                if !enabled.contains(&pass.source) {
                    continue;
                }
                if pass_spent(facts, word.word_id, pass.mark) {
                    continue;
                }
                jobs.push(
                    JobSpec::new(
                        JobKey::new(
                            JobKind::FetchImages,
                            SubjectRef::word_source(word.word_id, pass.mark),
                        ),
                        pass.rate_key,
                        // P2, like the pass it retries: this is backlog
                        // backfill, and it competes with the first passes of
                        // words further down the frequency list on the same
                        // terms they compete with each other.
                        Priority::P2,
                    )
                    .with_tiebreak(word.frequency_rank, word.word_id)
                    .with_payload(JobPayload::FetchImages {
                        word_id: word.word_id,
                        lemma: word.lemma.clone(),
                        source: pass.source,
                        gloss: facts.primary_gloss.get(&word.word_id).cloned(),
                        strategy: pass.strategy,
                        mark: pass.mark.to_string(),
                        gloss_tokens: facts
                            .primary_gloss_tokens
                            .get(&word.word_id)
                            .cloned()
                            .unwrap_or_default(),
                    }),
                );
                // One stage per word per pass. The next one derives when this
                // one has written its mark.
                break;
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
            // "Zero available candidates": a word that already has a picture of
            // its own from anywhere never reaches the generative fallback.
            if !facts.needs_image_candidates(word.word_id) {
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
            // And the second passes too: a real photograph under an awkward
            // licence still depicts the word, which nothing generated does.
            // This is what "exhaust the online sources first" means in code.
            let retries_spent = IMAGE_SECOND_PASSES.iter().all(|pass| {
                !enabled.contains(&pass.source) || pass_spent(facts, word.word_id, pass.mark)
            });
            if !retries_spent {
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
