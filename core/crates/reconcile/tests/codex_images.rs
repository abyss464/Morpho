//! The codex source: the last link in the image chain, and the only one that
//! fires on a picture being *inapt* rather than absent.
//!
//! Derivation tests. The real rule against real facts, asserting on the job set;
//! no socket, no generator, no quota spent anywhere.
//!
//! Three things are pinned here, and all three are the difference between a
//! fallback and a policy:
//!
//! * **position** — every library, both second passes and local SDXL come
//!   first, because a photograph of something that exists beats a picture of
//!   something that does not;
//! * **trigger** — the word's best CLIP score against its own sentence, which is
//!   the first trigger in the chain that can see a full pool of sharp,
//!   correctly-licensed pictures of the wrong thing and still say "this word is
//!   unserved";
//! * **conditioning** — a word with no slot-1 sentence is deferred, never drawn
//!   from its lemma. This source draws the scene a sentence describes, and that
//!   same sentence is what CLIP scores the answer against; conditioning on
//!   anything else would judge the result against a question it was never asked.

mod common;

use std::sync::Arc;

use common::{
    derive_as, harness, job_subjects, mark_fetched, seed_clip_score, seed_example, seed_image,
    seed_word, select_example, Harness,
};
use morpho_domain::types::{ExampleSource, ImageSource, MediaKind, Role};
use morpho_reconcile::rules::GenImageCodexRule;
use morpho_reconcile::sources::SourceSet;
use morpho_reconcile::{
    AdapterConfig, EngineContext, ImagesConfig, JobPayload, JobSpec, Rule, SourcesConfig,
};
use morpho_store::MediaStore;

const SENTENCE: &str = "She had to abandon the car in the flood.";
const PICTURE: &[u8] = b"a photograph of a power adapter";

/// Every library pass, strict and second, as `source_fetch.source` spells it —
/// plus SDXL, because generation waits for generation.
const CHAIN: &[&str] = &[
    "wikimedia",
    "openverse",
    "openverse_relaxed",
    "wikimedia_widened",
    "openverse_widened",
];

async fn exhaust_the_chain(fixture: &Harness, word_id: i64) {
    for source in CHAIN {
        mark_fetched(&fixture.store, "images", word_id, source, 0).await;
    }
    mark_fetched(&fixture.store, "images", word_id, "sdxl", 0).await;
}

/// The repository root, so the adapter probe finds `adapters/codex` wherever
/// cargo happens to have set the working directory.
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repository root")
}

/// A context in which the codex source is genuinely available.
///
/// Both halves of that have to be true — the adapter's project on disk and a
/// generator binary on `PATH` — so the helper asserts it rather than letting a
/// missing `uv` turn every assertion below into a vacuous pass.
fn engine(fixture: &Harness, images: ImagesConfig) -> Arc<EngineContext> {
    let sources = SourceSet::load(
        SourcesConfig {
            // `sh` stands in for the generator: the rule only asks whether one
            // exists, and running it is the executor's business.
            codex_bin: Some("sh".into()),
            ..SourcesConfig::default()
        },
        AdapterConfig {
            adapters_root: Some(repo_root()),
            ..AdapterConfig::default()
        },
    )
    .expect("source set");
    assert!(
        sources.has_codex(),
        "the codex adapter must be on disk and its launcher runnable for these \
         assertions to mean anything"
    );
    Arc::new(EngineContext::new(sources, MediaStore::new(fixture.dir.path())).with_images(images))
}

fn enabled() -> ImagesConfig {
    ImagesConfig {
        codex_enabled: true,
        ..ImagesConfig::default()
    }
}

async fn codex_jobs(fixture: &Harness, images: ImagesConfig) -> Vec<JobSpec> {
    let model_ver = images.clip_model_ver();
    let rule = Arc::new(GenImageCodexRule::new(engine(fixture, images))) as Arc<dyn Rule>;
    derive_as(&fixture.store, rule, &model_ver).await
}

fn payload_of(job: &JobSpec) -> (&str, &str) {
    match &job.payload {
        JobPayload::GenImageCodex {
            lemma, sentence, ..
        } => (lemma, sentence),
        other => panic!("unexpected payload: {other:?}"),
    }
}

/// A word the whole chain has failed: a sentence, one picture nobody could call
/// apt, and every library and SDXL spent.
async fn inapt_word(fixture: &Harness) -> i64 {
    let store = &fixture.store;
    let media = MediaStore::new(fixture.dir.path());
    let word_id = seed_word(store, "abandon", Role::Target).await;
    let cand = seed_example(
        store,
        word_id,
        "abandon",
        SENTENCE,
        ExampleSource::ExamCorpus,
    )
    .await;
    select_example(store, word_id, 1, cand).await;
    seed_image(store, &media, word_id, PICTURE, ImageSource::Wikimedia).await;
    let hash = media
        .put_bytes(PICTURE, MediaKind::Image)
        .unwrap()
        .file_hash;
    seed_clip_score(store, word_id, &hash, 0.11).await;
    exhaust_the_chain(fixture, word_id).await;
    word_id
}

// -- the switches ------------------------------------------------------------

/// Nothing happens until an operator says so. This source spends somebody
/// else's quota, so a checkout that has never heard of it derives nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_source_is_off_until_it_is_turned_on() {
    let fixture = harness();
    inapt_word(&fixture).await;
    assert!(codex_jobs(&fixture, ImagesConfig::default())
        .await
        .is_empty());
    assert!(!codex_jobs(&fixture, enabled()).await.is_empty());
}

/// The other half of the gate: an enabled source with no generator behind it is
/// *absent*, which derives nothing at all rather than dead-lettering every word.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_enabled_source_with_no_generator_derives_nothing() {
    let fixture = harness();
    inapt_word(&fixture).await;

    let sources = SourceSet::load(
        SourcesConfig {
            codex_bin: Some("/nonexistent/codex".into()),
            ..SourcesConfig::default()
        },
        AdapterConfig {
            adapters_root: Some(repo_root()),
            ..AdapterConfig::default()
        },
    )
    .expect("source set");
    assert!(!sources.has_codex());
    let context = Arc::new(
        EngineContext::new(sources, MediaStore::new(fixture.dir.path())).with_images(enabled()),
    );
    let rule = Arc::new(GenImageCodexRule::new(context)) as Arc<dyn Rule>;
    assert!(derive_as(&fixture.store, rule, &enabled().clip_model_ver())
        .await
        .is_empty());
}

// -- conditioning (owner ruling, wave 9) -------------------------------------

/// The ruling, as an assertion: no sentence, no generation.
///
/// The word is otherwise perfectly eligible — chain spent, picture inapt — and
/// still derives nothing, because what this source draws is the scene a
/// sentence describes. It becomes eligible the moment an example lands, with
/// nothing to clear.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_word_with_no_sentence_is_deferred_rather_than_drawn_from_its_lemma() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let word_id = seed_word(&store, "abandon", Role::Target).await;
    seed_image(&store, &media, word_id, PICTURE, ImageSource::Wikimedia).await;
    let hash = media
        .put_bytes(PICTURE, MediaKind::Image)
        .unwrap()
        .file_hash;
    seed_clip_score(&store, word_id, &hash, 0.11).await;
    exhaust_the_chain(&fixture, word_id).await;

    assert!(
        codex_jobs(&fixture, enabled()).await.is_empty(),
        "a word with no slot-1 sentence must never be generated for"
    );

    // The sentence arrives; the word becomes eligible on the very next pass.
    let cand = seed_example(
        &store,
        word_id,
        "abandon",
        SENTENCE,
        ExampleSource::ExamCorpus,
    )
    .await;
    select_example(&store, word_id, 1, cand).await;
    let jobs = codex_jobs(&fixture, enabled()).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(payload_of(&jobs[0]), ("abandon", SENTENCE));
}

/// The payload carries the sentence the rule saw, verbatim — the same text the
/// CLIP score that will judge the result was computed against.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_job_carries_the_sentence_the_score_will_be_measured_with() {
    let fixture = harness();
    let word_id = inapt_word(&fixture).await;
    let jobs = codex_jobs(&fixture, enabled()).await;
    assert_eq!(
        job_subjects(&jobs),
        vec![format!("gen_image_codex/{word_id}:codex_codex_1")]
    );
    let (lemma, sentence) = payload_of(&jobs[0]);
    assert_eq!((lemma, sentence), ("abandon", SENTENCE));
}

// -- the trigger -------------------------------------------------------------

/// A word whose picture is apt is left alone, however much quota is available.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apt_picture_is_not_replaced() {
    let fixture = harness();
    let word_id = inapt_word(&fixture).await;
    let media = MediaStore::new(fixture.dir.path());
    let hash = media
        .put_bytes(PICTURE, MediaKind::Image)
        .unwrap()
        .file_hash;
    // Rescored well above the threshold: the word is served.
    seed_clip_score(&fixture.store, word_id, &hash, 0.29).await;
    assert!(codex_jobs(&fixture, enabled()).await.is_empty());
}

/// The threshold is a knob, and it is the whole trigger: the same word with the
/// same picture is eligible or not depending on where it is set.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_threshold_decides() {
    let fixture = harness();
    let word_id = inapt_word(&fixture).await;
    let media = MediaStore::new(fixture.dir.path());
    let hash = media
        .put_bytes(PICTURE, MediaKind::Image)
        .unwrap()
        .file_hash;
    seed_clip_score(&fixture.store, word_id, &hash, 0.19).await;

    let low = ImagesConfig {
        codex_threshold: 0.15,
        ..enabled()
    };
    assert!(codex_jobs(&fixture, low).await.is_empty());
    let high = ImagesConfig {
        codex_threshold: 0.25,
        ..enabled()
    };
    assert_eq!(codex_jobs(&fixture, high).await.len(), 1);
}

/// A word with no picture at all qualifies too. It has nothing to score, which
/// is the same conclusion from the other end — and refusing it would make the
/// last link in the chain unreachable by the words that need it most.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_word_with_no_picture_at_all_qualifies() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "abandon", Role::Target).await;
    let cand = seed_example(
        &store,
        word_id,
        "abandon",
        SENTENCE,
        ExampleSource::ExamCorpus,
    )
    .await;
    select_example(&store, word_id, 1, cand).await;
    exhaust_the_chain(&fixture, word_id).await;
    assert_eq!(codex_jobs(&fixture, enabled()).await.len(), 1);
}

// -- position in the chain ---------------------------------------------------

/// A library that has not answered yet might still answer, and a real
/// photograph wins.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_is_generated_before_every_library_is_spent() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());
    let word_id = seed_word(&store, "abandon", Role::Target).await;
    let cand = seed_example(
        &store,
        word_id,
        "abandon",
        SENTENCE,
        ExampleSource::ExamCorpus,
    )
    .await;
    select_example(&store, word_id, 1, cand).await;
    seed_image(&store, &media, word_id, PICTURE, ImageSource::Wikimedia).await;
    let hash = media
        .put_bytes(PICTURE, MediaKind::Image)
        .unwrap()
        .file_hash;
    seed_clip_score(&store, word_id, &hash, 0.11).await;

    for (index, source) in CHAIN.iter().enumerate() {
        assert!(
            codex_jobs(&fixture, enabled()).await.is_empty(),
            "generated with {} of {} library passes spent",
            index,
            CHAIN.len()
        );
        mark_fetched(&store, "images", word_id, source, 0).await;
    }
    // Every library is spent; SDXL is not configured in this context, so the
    // chain is at its end and the word is finally eligible.
    assert_eq!(codex_jobs(&fixture, enabled()).await.len(), 1);
}

/// One generation per word per prompt version. The mark the job writes is the
/// mark the next derivation checks, so a word is never asked for twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_word_is_generated_for_once_per_prompt_version() {
    let fixture = harness();
    let word_id = inapt_word(&fixture).await;
    assert_eq!(codex_jobs(&fixture, enabled()).await.len(), 1);

    mark_fetched(&fixture.store, "images", word_id, "codex_codex_1", 1).await;
    assert!(codex_jobs(&fixture, enabled()).await.is_empty());

    // Bumping the template moves the subject, which is how an operator asks for
    // the pass again without clearing a mark or resetting a dead letter.
    let bumped = ImagesConfig {
        codex_prompt_ver: morpho_reconcile::config::CodexPromptVer("codex/2".into()),
        ..enabled()
    };
    assert_eq!(
        job_subjects(&codex_jobs(&fixture, bumped).await),
        vec![format!("gen_image_codex/{word_id}:codex_codex_2")]
    );
}

/// The batch cap. A first run over a whole lexicon must not put six thousand
/// generation requests in front of somebody's rate limit; the queue is derived,
/// so the words this pass skips simply come back next pass.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_pass_asks_for_at_most_a_batch() {
    let fixture = harness();
    let store = fixture.store.clone();
    for index in 0..6 {
        let lemma = format!("word{index}");
        let word_id = seed_word(&store, &lemma, Role::Target).await;
        let sentence = format!("A sentence about {lemma} in the flood.");
        let cand = seed_example(
            &store,
            word_id,
            &lemma,
            &sentence,
            ExampleSource::ExamCorpus,
        )
        .await;
        select_example(&store, word_id, 1, cand).await;
        exhaust_the_chain(&fixture, word_id).await;
    }
    let capped = ImagesConfig {
        codex_batch: 2,
        ..enabled()
    };
    assert_eq!(codex_jobs(&fixture, capped).await.len(), 2);
    assert_eq!(codex_jobs(&fixture, enabled()).await.len(), 6);
}
