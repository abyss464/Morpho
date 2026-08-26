//! Scene-image mode: the generative tier prompted with the word's own sentence.
//!
//! These are derivation tests. They run the real rule against real facts and
//! assert on the job set, with no socket and no GPU anywhere near them.
//!
//! What they pin is the *position* of the tier as much as its contents. Scene
//! mode is a quality upgrade to the last link in the image chain, not a new
//! first link: every library is still asked, still asked again on looser terms,
//! and only a word none of them could answer for is generated for at all. A
//! test that let a scene job derive before the libraries were spent would be
//! describing a different product.

mod common;

use std::sync::Arc;

use common::{
    derive, harness, job_subjects, mark_fetched, seed_example, seed_image, seed_image_with_ref,
    seed_word, select_example, select_image, Harness,
};
use morpho_domain::event::Actor;
use morpho_domain::types::{ExampleSource, GlossSource, ImageSource, Role};
use morpho_reconcile::rules::{FetchImagesSecondPassRule, GenImageSdxlRule, GenSceneImageRule};
use morpho_reconcile::sources::SourceSet;
use morpho_reconcile::{
    AdapterConfig, EngineContext, ImagesConfig, JobPayload, JobSpec, Rule, ScenePrompt,
    SourcesConfig,
};
use morpho_store::ops::SetGloss;
use morpho_store::{MediaStore, Store, WriteOp};

const SENTENCE: &str = "She had to abandon the car in the flood.";

/// Every strict image pass, as `source_fetch.source` holds it, followed by
/// every second pass. A word that carries all five has been everywhere the
/// libraries go.
const ONLINE_CHAIN: &[&str] = &[
    "wikimedia",
    "openverse",
    "openverse_relaxed",
    "wikimedia_widened",
    "openverse_widened",
];

/// Mark every library pass as answered and empty, which is the only state from
/// which anything is ever generated.
async fn exhaust_the_libraries(store: &Store, word_id: i64) {
    for source in ONLINE_CHAIN {
        mark_fetched(store, "images", word_id, source, 0).await;
    }
}

fn engine(fixture: &Harness, images: ImagesConfig) -> Arc<EngineContext> {
    let sources = SourceSet::load(
        SourcesConfig {
            comfyui_url: Some("http://127.0.0.1:8188".into()),
            ..SourcesConfig::default()
        },
        AdapterConfig::default(),
    )
    .expect("source set");
    Arc::new(EngineContext::new(sources, MediaStore::new(fixture.dir.path())).with_images(images))
}

fn scene_mode() -> ImagesConfig {
    ImagesConfig {
        scene_mode: true,
        ..ImagesConfig::default()
    }
}

async fn scene_jobs(fixture: &Harness, images: ImagesConfig) -> Vec<JobSpec> {
    let rule = Arc::new(GenSceneImageRule::new(engine(fixture, images))) as Arc<dyn Rule>;
    derive(&fixture.store, rule).await
}

async fn bare_jobs(fixture: &Harness, images: ImagesConfig) -> Vec<JobSpec> {
    let rule = Arc::new(GenImageSdxlRule::new(engine(fixture, images))) as Arc<dyn Rule>;
    derive(&fixture.store, rule).await
}

/// The scene the payload asks for, if this job asks for one.
fn scene_of(job: &JobSpec) -> Option<&ScenePrompt> {
    match &job.payload {
        JobPayload::GenImageSdxl { scene, .. } => scene.as_ref(),
        _ => None,
    }
}

/// One word with a selected slot-1 sentence and no picture anybody could find.
async fn stranded_word(fixture: &Harness) -> i64 {
    let store = &fixture.store;
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
    exhaust_the_libraries(store, word_id).await;
    word_id
}

// -- the switch --------------------------------------------------------------

/// Nothing changes until an operator says so. A checkout that has never heard
/// of scene mode derives exactly the jobs it derived before it existed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scene_mode_off_derives_nothing_and_leaves_the_bare_rule_alone() {
    let fixture = harness();
    let word_id = stranded_word(&fixture).await;

    assert!(scene_jobs(&fixture, ImagesConfig::default())
        .await
        .is_empty());

    let bare = bare_jobs(&fixture, ImagesConfig::default()).await;
    assert_eq!(
        job_subjects(&bare),
        vec![format!("gen_image_sdxl/{word_id}:sdxl")]
    );
    assert!(scene_of(&bare[0]).is_none());
}

/// With no ComfyUI there is nothing to generate with, whatever the switch says.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scene_mode_without_a_backend_derives_nothing() {
    let fixture = harness();
    stranded_word(&fixture).await;

    let sources =
        SourceSet::load(SourcesConfig::default(), AdapterConfig::default()).expect("source set");
    let context = Arc::new(
        EngineContext::new(sources, MediaStore::new(fixture.dir.path())).with_images(scene_mode()),
    );
    let rule = Arc::new(GenSceneImageRule::new(context)) as Arc<dyn Rule>;
    assert!(derive(&fixture.store, rule).await.is_empty());
}

// -- position in the chain ---------------------------------------------------

/// The gate that makes this a fallback rather than a policy: a library that has
/// not answered yet might still answer, and a real photograph wins.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scene_job_waits_for_every_library_to_be_spent() {
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

    // One pass at a time: nothing derives until the last mark lands.
    for source in ONLINE_CHAIN {
        assert!(
            scene_jobs(&fixture, scene_mode()).await.is_empty(),
            "{source} had not been asked yet"
        );
        mark_fetched(&store, "images", word_id, source, 0).await;
    }
    assert_eq!(scene_jobs(&fixture, scene_mode()).await.len(), 1);
}

/// A word a library could serve is served by the library. Generation never
/// competes with a photograph — it only covers what the photograph could not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_word_with_a_library_photograph_is_never_generated_for() {
    let fixture = harness();
    let word_id = stranded_word(&fixture).await;
    let media = MediaStore::new(fixture.dir.path());
    seed_image(
        &fixture.store,
        &media,
        word_id,
        b"a real photograph",
        ImageSource::Wikimedia,
    )
    .await;

    assert!(scene_jobs(&fixture, scene_mode()).await.is_empty());
    assert!(bare_jobs(&fixture, scene_mode()).await.is_empty());
}

/// The wave-8 widening still applies underneath: a word whose only photograph
/// is one another word already shows has, for this purpose, none — so it walks
/// on to the generative tier like any word nobody answered for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_word_holding_only_somebody_elses_photograph_still_reaches_the_tier() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let owner = seed_word(&store, "adapt", Role::Target).await;
    let owned = seed_image(&store, &media, owner, b"one photo", ImageSource::Wikimedia).await;
    select_image(&store, owner, owned).await;

    let word_id = stranded_word(&fixture).await;
    seed_image(
        &store,
        &media,
        word_id,
        b"one photo",
        ImageSource::Wikimedia,
    )
    .await;

    assert_eq!(
        job_subjects(&scene_jobs(&fixture, scene_mode()).await),
        vec![format!("gen_image_sdxl/{word_id}:sdxl_scene_1")]
    );
}

// -- what the rule needs -----------------------------------------------------

/// No sentence, no scene. The word is not abandoned: the bare-concept rule
/// takes it, because a picture of the idea beats no picture at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_word_without_a_slot_one_sentence_falls_back_to_the_bare_prompt() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "abandon", Role::Target).await;
    // A candidate nothing selected is not a slot-1 sentence.
    seed_example(
        &store,
        word_id,
        "abandon",
        SENTENCE,
        ExampleSource::ExamCorpus,
    )
    .await;
    exhaust_the_libraries(&store, word_id).await;

    assert!(scene_jobs(&fixture, scene_mode()).await.is_empty());
    assert_eq!(
        job_subjects(&bare_jobs(&fixture, scene_mode()).await),
        vec![format!("gen_image_sdxl/{word_id}:sdxl")]
    );
}

/// The two rules never both take the same word: with scene mode on, a word that
/// owns a sentence belongs to the scene rule and the bare rule stands off.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_two_generative_rules_never_derive_for_the_same_word() {
    let fixture = harness();
    let word_id = stranded_word(&fixture).await;

    let scene = scene_jobs(&fixture, scene_mode()).await;
    let bare = bare_jobs(&fixture, scene_mode()).await;
    assert_eq!(
        job_subjects(&scene),
        vec![format!("gen_image_sdxl/{word_id}:sdxl_scene_1")]
    );
    assert!(bare.is_empty(), "{:?}", job_subjects(&bare));
}

/// A gloss anchor is a terminator of the readability chain, not a word anybody
/// learns. It takes no assets — least of all a generated one (ruling #18a).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_anchored_word_is_never_generated_for() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = stranded_word(&fixture).await;
    assert_eq!(scene_jobs(&fixture, scene_mode()).await.len(), 1);

    store
        .write(
            Actor::admin("abyss"),
            WriteOp::SetGloss(SetGloss {
                word_id,
                zh_gloss: Some("放弃".to_string()),
                source: GlossSource::Manual,
            }),
        )
        .await
        .unwrap();

    assert!(scene_jobs(&fixture, scene_mode()).await.is_empty());
    assert!(bare_jobs(&fixture, scene_mode()).await.is_empty());
}

// -- one job per word per template version -----------------------------------

/// A scene image already in hand ends the derivation. Nothing re-generates on
/// the next pass, which is the difference between a fallback and a treadmill.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_word_that_already_holds_a_current_scene_image_derives_nothing() {
    let fixture = harness();
    let word_id = stranded_word(&fixture).await;
    let media = MediaStore::new(fixture.dir.path());
    seed_image_with_ref(
        &fixture.store,
        &media,
        word_id,
        b"a generated scene",
        ImageSource::Sdxl,
        "sdxl:1234 (scene scene/1)",
    )
    .await;

    assert!(scene_jobs(&fixture, scene_mode()).await.is_empty());
}

/// Bumping the template version is how an operator asks for the whole lexicon
/// again. The job subject moves with the version, so the pass derives against a
/// subject that has never run — no mark to clear, no dead letter to reset.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_template_bump_re_derives_a_word_that_already_has_a_scene_image() {
    let fixture = harness();
    let word_id = stranded_word(&fixture).await;
    let media = MediaStore::new(fixture.dir.path());
    seed_image_with_ref(
        &fixture.store,
        &media,
        word_id,
        b"a generated scene",
        ImageSource::Sdxl,
        "sdxl:1234 (scene scene/1)",
    )
    .await;

    let bumped = ImagesConfig {
        scene_prompt_ver: morpho_reconcile::config::ScenePromptVer("scene/2".into()),
        ..scene_mode()
    };
    let jobs = scene_jobs(&fixture, bumped).await;
    assert_eq!(
        job_subjects(&jobs),
        vec![format!("gen_image_sdxl/{word_id}:sdxl_scene_2")]
    );
    assert_eq!(scene_of(&jobs[0]).unwrap().prompt_ver, "scene/2");
}

/// A generation made before scene mode existed is not a scene image, so the
/// word is still owed one — it has no library picture, and the old candidate
/// says nothing about the sentence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_old_bare_generation_does_not_satisfy_the_scene_rule() {
    let fixture = harness();
    let word_id = stranded_word(&fixture).await;
    let media = MediaStore::new(fixture.dir.path());
    seed_image_with_ref(
        &fixture.store,
        &media,
        word_id,
        b"a bare-concept generation",
        ImageSource::Sdxl,
        r#"{"prompt":"a clear photographic scene","seed":1234,"model":"sdxl"}"#,
    )
    .await;

    assert_eq!(
        job_subjects(&scene_jobs(&fixture, scene_mode()).await),
        vec![format!("gen_image_sdxl/{word_id}:sdxl_scene_1")]
    );
}

// -- the payload -------------------------------------------------------------

/// The sentence travels with the job rather than being looked up again, so the
/// sentence the rule saw and the sentence the prompt describes cannot drift
/// apart across a pass boundary.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_payload_carries_the_sentence_the_rule_saw() {
    let fixture = harness();
    let word_id = stranded_word(&fixture).await;

    let jobs = scene_jobs(&fixture, scene_mode()).await;
    let JobPayload::GenImageSdxl {
        word_id: payload_word,
        lemma,
        scene,
        ..
    } = &jobs[0].payload
    else {
        panic!("wrong payload: {:?}", jobs[0].payload);
    };
    assert_eq!(*payload_word, word_id);
    assert_eq!(lemma, "abandon");
    let scene = scene.as_ref().expect("a scene job carries a scene");
    assert_eq!(scene.sentence, SENTENCE);
    assert_eq!(scene.prompt_ver, "scene/1");
}

/// Slot 1 is the mode-1 sentence and the only one the card shows beside the
/// picture. Slots 2 and 3 are extra reading, and a picture of one of those
/// would illustrate something the learner is not looking at.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_the_slot_one_sentence_becomes_a_scene() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "abandon", Role::Target).await;
    let second = seed_example(
        &store,
        word_id,
        "abandon",
        "They abandon the search at dusk.",
        ExampleSource::Tatoeba,
    )
    .await;
    select_example(&store, word_id, 2, second).await;
    exhaust_the_libraries(&store, word_id).await;

    assert!(scene_jobs(&fixture, scene_mode()).await.is_empty());

    let first = seed_example(
        &store,
        word_id,
        "abandon",
        SENTENCE,
        ExampleSource::ExamCorpus,
    )
    .await;
    select_example(&store, word_id, 1, first).await;

    let jobs = scene_jobs(&fixture, scene_mode()).await;
    assert_eq!(scene_of(&jobs[0]).unwrap().sentence, SENTENCE);
}

// -- against the wave-8 duplicate pressure -----------------------------------

/// A scene image is a function of one word's own seed, so it is per-word unique
/// by construction and the duplicate-image reopen has no business firing on it.
///
/// That reopen exists to walk a word whose only photograph is somebody else's
/// back down the library chain until something unshared turns up. A word whose
/// picture was generated for it has no such chain left to walk, and reopening
/// one would spend a request per pass, forever, on libraries that already said
/// no. So the exemption is what keeps a settled word settled.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scene_image_never_reopens_the_library_chain() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    // Contrived, because two words cannot really land on the same generated
    // bytes: one word owns the picture, another holds a candidate for it.
    let owner = seed_word(&store, "adapt", Role::Target).await;
    let owned = seed_image_with_ref(
        &store,
        &media,
        owner,
        b"a generated scene",
        ImageSource::Sdxl,
        "sdxl:1234 (scene scene/1)",
    )
    .await;
    select_image(&store, owner, owned).await;

    let word_id = seed_word(&store, "abandon", Role::Target).await;
    for source in ["wikimedia", "openverse"] {
        mark_fetched(&store, "images", word_id, source, 0).await;
    }
    seed_image_with_ref(
        &store,
        &media,
        word_id,
        b"a generated scene",
        ImageSource::Sdxl,
        "sdxl:1234 (scene scene/1)",
    )
    .await;

    let rule = Arc::new(FetchImagesSecondPassRule::new(engine(
        &fixture,
        scene_mode(),
    ))) as Arc<dyn Rule>;
    let jobs = derive(&store, rule).await;
    assert!(jobs.is_empty(), "{:?}", job_subjects(&jobs));
}

/// The pressure itself is untouched: a *photograph* another word already shows
/// still reopens the chain, which is the wave-8 behaviour this must not break.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shared_photograph_still_reopens_the_library_chain() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let owner = seed_word(&store, "adapt", Role::Target).await;
    let owned = seed_image(&store, &media, owner, b"one photo", ImageSource::Wikimedia).await;
    select_image(&store, owner, owned).await;

    let word_id = seed_word(&store, "abandon", Role::Target).await;
    for source in ["wikimedia", "openverse"] {
        mark_fetched(&store, "images", word_id, source, 0).await;
    }
    seed_image(
        &store,
        &media,
        word_id,
        b"one photo",
        ImageSource::Wikimedia,
    )
    .await;

    let rule = Arc::new(FetchImagesSecondPassRule::new(engine(
        &fixture,
        scene_mode(),
    ))) as Arc<dyn Rule>;
    assert_eq!(
        job_subjects(&derive(&store, rule).await),
        vec![format!("fetch_images/{word_id}:openverse_relaxed")]
    );
}

/// The expensive tier stays at the back of the queue, and words come off it in
/// frequency order like everything else.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scene_jobs_ride_the_generative_lane_at_the_lowest_priority() {
    let fixture = harness();
    stranded_word(&fixture).await;

    let jobs = scene_jobs(&fixture, scene_mode()).await;
    assert_eq!(jobs[0].rate_key, morpho_domain::job::RateKey::Sdxl);
    assert_eq!(jobs[0].priority, morpho_domain::job::Priority::P3);
}
