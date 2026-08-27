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
use crate::rule::{JobPayload, JobSpec, Rule, ScenePrompt, Snapshot};
use crate::score::ImageStrategy;
use crate::sources::clip::MAX_IMAGES_PER_REQUEST;

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

/// Has every library been asked, first pass and second passes alike?
///
/// This is the gate on the whole generative tier, and it does not move: a real
/// photograph of something that exists beats a picture of something that does
/// not, however the latter was prompted. Only a word every library came back
/// empty for reaches generation at all.
fn libraries_spent(facts: &Facts, enabled: &[ImageSource], word_id: i64) -> bool {
    let strict = IMAGE_PROVIDERS.iter().all(|provider| {
        // Not enabled => permanently absent => spent.
        !enabled.contains(&provider.source)
            || facts.source_exhausted(
                &FetchKind::Images,
                JobKind::FetchImages,
                word_id,
                image_source_name(provider.source),
            )
    });
    strict
        && IMAGE_SECOND_PASSES
            .iter()
            .all(|pass| !enabled.contains(&pass.source) || pass_spent(facts, word_id, pass.mark))
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

        let scene_mode = self.context.images.scene_mode;

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
            // With scene mode on, a word that owns a sentence belongs to the
            // scene rule instead. Words without one still come through here:
            // there is nothing to describe, and a bare-concept picture is
            // better than no picture.
            if scene_mode && facts.slot_one_example.contains_key(&word.word_id) {
                continue;
            }
            // Every library, first pass and second passes alike, has to be
            // spent first: a real photograph under an awkward licence still
            // depicts the word, which nothing generated does. This is what
            // "exhaust the online sources first" means in code.
            if !libraries_spent(facts, &enabled, word.word_id) {
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
                    scene: None,
                }),
            );
        }
        Ok(jobs)
    }
}

/// The generative tier, prompted with the word's own sentence.
///
/// Same position in the chain as [`GenImageSdxlRule`] and the same gate: every
/// library asked, every second pass spent, nothing usable to show for it. What
/// differs is the ask. A bare-concept prompt gives a learner a picture *of a
/// word*, which for anything abstract is a stock-art abstraction nobody can
/// answer a quiz on; a prompt built from the sentence the card already shows
/// gives them a picture of the moment that sentence describes, and the card
/// becomes one scene read two ways.
///
/// A word reaches this rule once per prompt-template version. Bumping
/// `scene_prompt_ver` moves the job subject, so the pass re-derives against a
/// subject that has never run — no mark to clear, no dead letter to reset.
pub struct GenSceneImageRule {
    context: Arc<EngineContext>,
}

impl GenSceneImageRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for GenSceneImageRule {
    fn name(&self) -> &'static str {
        "gen_scene_image"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        if !self.context.images.scene_mode || !self.context.sources.has_sdxl() {
            return Ok(Vec::new());
        }
        let facts = snapshot.facts;
        let enabled = self.context.sources.config.enabled_image_sources();
        let prompt_ver = self.context.images.scene_prompt_ver();
        let mark = self.context.images.scene_mark();

        let mut jobs = Vec::new();
        for word in &facts.active {
            // `active` already excludes gloss anchors; saying so here as well
            // costs a pointer comparison and keeps the rule honest if the fact
            // set ever widens (admin-api.md ruling #18a).
            if word.zh_gloss.is_some() {
                continue;
            }
            // Generation is the last resort, not the first: a word any library
            // could serve is served by the library.
            if !facts.needs_library_image(word.word_id) {
                continue;
            }
            if facts.has_scene_image(word.word_id, prompt_ver) {
                continue;
            }
            if !libraries_spent(facts, &enabled, word.word_id) {
                continue;
            }
            let Some(sentence) = facts.slot_one_example.get(&word.word_id) else {
                // No sentence, no scene. The bare-concept rule takes the word.
                continue;
            };
            jobs.push(
                JobSpec::new(
                    JobKey::new(
                        JobKind::GenImageSdxl,
                        SubjectRef::word_source(word.word_id, &mark),
                    ),
                    RateKey::Sdxl,
                    Priority::P3,
                )
                .with_tiebreak(word.frequency_rank, word.word_id)
                .with_payload(JobPayload::GenImageSdxl {
                    word_id: word.word_id,
                    lemma: word.lemma.clone(),
                    gloss: facts.primary_gloss.get(&word.word_id).cloned(),
                    scene: Some(ScenePrompt {
                        sentence: sentence.clone(),
                        prompt_ver: prompt_ver.to_string(),
                    }),
                }),
            );
        }
        Ok(jobs)
    }
}

/// Score a word's pictures against the sentence its card shows.
///
/// The desired state is one row in `clip_scores` per `(available candidate,
/// word's query, current model)`. Because the key is the *comparison* and not
/// the candidate, the work shrinks on its own: two words that ended up with the
/// same stock photo and the same sentence need one score between them, a word
/// that re-selects a sentence it once had needs none at all, and a rejected
/// candidate takes nothing with it.
///
/// A word derives a job when anything in its pool is uncompared. There is no
/// completion mark and none is wanted: the rows themselves are the mark, and a
/// mark would have to be invalidated every time slot 1 moved. What a word does
/// get is the ordinary `job_state` treatment — a sidecar that keeps failing
/// backs the word off and eventually dead-letters it, and selection carries on
/// ranking that word on quality alone.
pub struct ScoreImageClipRule {
    context: Arc<EngineContext>,
}

impl ScoreImageClipRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for ScoreImageClipRule {
    fn name(&self) -> &'static str {
        "score_image_clip"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        if !self.context.sources.has_clip() {
            return Ok(Vec::new());
        }
        let facts = snapshot.facts;
        let mut jobs = Vec::new();
        for word in &facts.active {
            // A gloss anchor is never learned, so it is never shown a picture.
            if word.zh_gloss.is_some() {
                continue;
            }
            let Some(query) = facts.clip_query.get(&word.word_id) else {
                continue;
            };
            let mut pending = facts.unscored_images(word.word_id);
            if pending.is_empty() {
                continue;
            }
            // One request per word per pass. A pool larger than the sidecar's
            // batch is scored a slice at a time, and the next pass asks for the
            // rest — the same way the second-pass chain walks one stage at a
            // time.
            pending.truncate(MAX_IMAGES_PER_REQUEST);
            jobs.push(
                JobSpec::new(
                    JobKey::new(JobKind::ScoreImageClip, SubjectRef::word(word.word_id)),
                    RateKey::Clip,
                    // P2: backfilling the semantic evidence for a library that
                    // already exists, ordered by frequency like every other
                    // backfill.
                    Priority::P2,
                )
                .with_tiebreak(word.frequency_rank, word.word_id)
                .with_payload(JobPayload::ScoreImageClip {
                    word_id: word.word_id,
                    text: query.text.clone(),
                    text_hash: query.text_hash.clone(),
                    file_hashes: pending,
                }),
            );
        }
        Ok(jobs)
    }
}

/// The last image source: generate a picture for a word nothing depicts aptly.
///
/// Every other tier fires on *absence* — a word with no candidate at all. This
/// one fires on **inaptness**, which is the whole reason it exists: the words
/// the 2026-08 wave generated for had full pools of sharp, correctly licensed
/// photographs of the wrong thing, and no rule that counts candidates can see
/// that. CLIP can, so the trigger is the word's best score against its own
/// sentence, and a word with nothing at all — nothing to score, no score —
/// qualifies for the same reason it qualifies everywhere else.
///
/// It sits behind SDXL rather than beside it. A locally generated picture costs
/// an evening of a GPU that is already paid for; this one costs somebody's
/// hosted quota, so it is only asked for what the whole rest of the chain, local
/// generation included, has failed to serve.
///
/// Three things keep it conservative: it is off unless an operator turns it on,
/// it is off unless its adapter is on disk, and one pass asks for at most
/// [`crate::config::ImagesConfig::codex_batch`] words. The queue is derived, so
/// the words it does not ask for this pass are simply asked for next pass.
pub struct GenImageCodexRule {
    context: Arc<EngineContext>,
}

impl GenImageCodexRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for GenImageCodexRule {
    fn name(&self) -> &'static str {
        "gen_image_codex"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        let images = &self.context.images;
        if !images.codex_enabled || !self.context.sources.has_codex() {
            return Ok(Vec::new());
        }
        let facts = snapshot.facts;
        let enabled = self.context.sources.config.enabled_image_sources();
        let mark = images.codex_mark();
        let sdxl_name = image_source_name(ImageSource::Sdxl);

        let mut jobs = Vec::new();
        for word in &facts.active {
            if word.zh_gloss.is_some() {
                continue;
            }
            if jobs.len() >= images.codex_batch {
                break;
            }
            // One generation per word per prompt version, ever. Bumping the
            // version moves the subject, which is how an operator asks for the
            // pass again without clearing a mark or resetting a dead letter.
            if facts.fetched(&FetchKind::Images, word.word_id, &mark) {
                continue;
            }
            // Everything cheaper first: every library, both second passes, and
            // local generation. `libraries_spent` covers the libraries;
            // SDXL is spent when it has answered or will not.
            if !libraries_spent(facts, &enabled, word.word_id) {
                continue;
            }
            if self.context.sources.has_sdxl() && !generation_spent(facts, word.word_id, sdxl_name)
            {
                continue;
            }
            if !inapt(facts, word.word_id, images.codex_threshold) {
                continue;
            }
            jobs.push(
                JobSpec::new(
                    JobKey::new(
                        JobKind::GenImageCodex,
                        SubjectRef::word_source(word.word_id, &mark),
                    ),
                    RateKey::Codex,
                    // P3, with SDXL: the expensive generative fallbacks.
                    Priority::P3,
                )
                .with_tiebreak(word.frequency_rank, word.word_id)
                .with_payload(JobPayload::GenImageCodex {
                    word_id: word.word_id,
                    lemma: word.lemma.clone(),
                    pos: facts.primary_pos.get(&word.word_id).cloned(),
                    gloss: facts.primary_gloss.get(&word.word_id).cloned(),
                    sentence: facts.slot_one_example.get(&word.word_id).cloned(),
                    prompt_ver: images.codex_prompt_ver().to_string(),
                    mark: mark.clone(),
                }),
            );
        }
        Ok(jobs)
    }
}

/// Has the local generator had its turn on this word?
///
/// Both scene mode and the bare-concept rule write their marks into the same
/// `source_fetch` family, and either one having run — or died, or been waived —
/// means SDXL has said what it has to say about this word.
fn generation_spent(facts: &Facts, word_id: i64, sdxl_name: &str) -> bool {
    let bare = pass_spent(facts, word_id, sdxl_name);
    let scene = facts
        .scene_image_vers
        .get(&word_id)
        .is_some_and(|vers| !vers.is_empty());
    bare || scene
}

/// Is the best picture this word has a poor answer to its own sentence?
///
/// Two ways to qualify, and they are the same condition seen from either end:
/// no candidate has a score — a word with an empty pool, or one the sidecar has
/// not reached yet — or the best score there is falls below the threshold.
///
/// An unscored word is admitted rather than skipped because the alternative is
/// worse in exactly the case that matters: a word with no pictures at all has
/// nothing to score, and refusing to generate for it would leave the last link
/// in the chain unreachable by the words that need it most.
fn inapt(facts: &Facts, word_id: i64, threshold: f64) -> bool {
    match facts.best_clip_score(word_id) {
        Some(best) => best < threshold,
        None => true,
    }
}
