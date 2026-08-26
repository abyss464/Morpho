//! Which sources a pass asks, and in what order it gives up on them
//! (admin-api.md ruling #18).
//!
//! These are derivation tests: they run the real rules against real facts and
//! assert on the job set, without a socket in sight. What they pin is the shape
//! of the fallback chain — keyed stock libraries only when configured, the two
//! open collections always, and the generative fallback strictly last.

mod common;

use std::sync::Arc;

use common::{derive, harness, job_subjects, mark_fetched, seed_image, seed_word};
use morpho_domain::types::{ImageSource, Role};
use morpho_reconcile::rules::{FetchExamplesRule, FetchImagesRule, GenImageSdxlRule};
use morpho_reconcile::sources::SourceSet;
use morpho_reconcile::{AdapterConfig, EngineContext, Rule, SourcesConfig};
use morpho_store::MediaStore;

/// Marker kinds, spelled as `source_fetch.kind` holds them.
const IMAGES: &str = "images";
const DEFINITIONS: &str = "definitions";

fn engine(data_dir: &std::path::Path, sources: SourcesConfig) -> Arc<EngineContext> {
    let set = SourceSet::load(sources, AdapterConfig::default()).expect("source set");
    Arc::new(EngineContext::new(set, MediaStore::new(data_dir)))
}

/// Every stock key set, plus a ComfyUI endpoint.
fn fully_credentialed() -> SourcesConfig {
    SourcesConfig {
        unsplash_access_key: Some("k".into()),
        pexels_api_key: Some("k".into()),
        pixabay_api_key: Some("k".into()),
        comfyui_url: Some("http://127.0.0.1:8188".into()),
        ..SourcesConfig::default()
    }
}

/// No credentials of any kind, but ComfyUI present — the configuration ruling
/// #18 is really about.
fn keyless_with_sdxl() -> SourcesConfig {
    SourcesConfig {
        comfyui_url: Some("http://127.0.0.1:8188".into()),
        ..SourcesConfig::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_keyless_install_still_asks_two_image_libraries() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;

    let context = engine(fixture.dir.path(), SourcesConfig::default());
    let jobs = derive(
        &store,
        Arc::new(FetchImagesRule::new(context)) as Arc<dyn Rule>,
    )
    .await;

    assert_eq!(
        job_subjects(&jobs),
        vec![
            format!("fetch_images/{word_id}:openverse"),
            format!("fetch_images/{word_id}:wikimedia"),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_keys_add_the_stock_libraries_without_replacing_the_open_ones() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;

    let context = engine(fixture.dir.path(), fully_credentialed());
    let jobs = derive(
        &store,
        Arc::new(FetchImagesRule::new(context)) as Arc<dyn Rule>,
    )
    .await;

    assert_eq!(
        job_subjects(&jobs),
        vec![
            format!("fetch_images/{word_id}:openverse"),
            format!("fetch_images/{word_id}:pexels"),
            format!("fetch_images/{word_id}:pixabay"),
            format!("fetch_images/{word_id}:unsplash"),
            format!("fetch_images/{word_id}:wikimedia"),
        ]
    );
}

/// Each provider rides its own dispatcher lane, so one library throttling
/// cannot stall the others.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_image_provider_gets_its_own_lane() {
    let fixture = harness();
    let store = fixture.store.clone();
    seed_word(&store, "serene", Role::Target).await;

    let context = engine(fixture.dir.path(), fully_credentialed());
    let jobs = derive(
        &store,
        Arc::new(FetchImagesRule::new(context)) as Arc<dyn Rule>,
    )
    .await;

    let mut lanes: Vec<String> = jobs.iter().map(|job| job.rate_key.to_string()).collect();
    lanes.sort();
    assert_eq!(
        lanes,
        vec!["openverse", "pexels", "pixabay", "unsplash", "wikimedia"]
    );
}

/// The heart of ruling #18: a missing stock key is no longer a reason to
/// generate a picture. SDXL waits for the keyless libraries to actually answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sdxl_waits_for_the_keyless_libraries_to_answer() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;

    let context = engine(fixture.dir.path(), keyless_with_sdxl());
    let rule = || Arc::new(GenImageSdxlRule::new(context.clone())) as Arc<dyn Rule>;

    assert!(
        derive(&store, rule()).await.is_empty(),
        "nothing has been asked yet"
    );

    mark_fetched(&store, IMAGES, word_id, "wikimedia", 0).await;
    assert!(
        derive(&store, rule()).await.is_empty(),
        "openverse has not answered"
    );

    mark_fetched(&store, IMAGES, word_id, "openverse", 0).await;
    assert_eq!(
        job_subjects(&derive(&store, rule()).await),
        vec![format!("gen_image_sdxl/{word_id}:sdxl")],
        "both open libraries came back empty; now it may generate"
    );
}

/// A keyed library that was never configured is spent from the start, so its
/// absence does not hold the fallback open forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unconfigured_stock_library_never_blocks_the_fallback() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;
    for source in ["wikimedia", "openverse"] {
        mark_fetched(&store, IMAGES, word_id, source, 0).await;
    }

    // Unsplash has a key and has not answered: the fallback stays shut.
    let keyed = engine(
        fixture.dir.path(),
        SourcesConfig {
            unsplash_access_key: Some("k".into()),
            ..keyless_with_sdxl()
        },
    );
    assert!(derive(
        &store,
        Arc::new(GenImageSdxlRule::new(keyed)) as Arc<dyn Rule>
    )
    .await
    .is_empty());

    // Same facts, no key: unsplash is absent rather than pending.
    let keyless = engine(fixture.dir.path(), keyless_with_sdxl());
    assert_eq!(
        derive(
            &store,
            Arc::new(GenImageSdxlRule::new(keyless)) as Arc<dyn Rule>
        )
        .await
        .len(),
        1
    );
}

/// "Zero available candidates" is the other half of the gate: a word that has a
/// picture from anywhere never reaches the generator, however exhausted the
/// libraries are.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_word_that_already_has_a_picture_is_never_generated_for() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;
    let media = MediaStore::new(fixture.dir.path());
    seed_image(
        &store,
        &media,
        word_id,
        b"pretend webp bytes",
        ImageSource::Wikimedia,
    )
    .await;
    for source in ["wikimedia", "openverse"] {
        mark_fetched(&store, IMAGES, word_id, source, 0).await;
    }

    let context = engine(fixture.dir.path(), keyless_with_sdxl());
    assert!(derive(
        &store,
        Arc::new(GenImageSdxlRule::new(context)) as Arc<dyn Rule>
    )
    .await
    .is_empty());
}

/// Without ComfyUI there is no generative fallback at all, and the word ends up
/// honestly reporting `missing_image` rather than accruing dead letters.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_comfyui_means_no_generative_fallback() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;
    for source in ["wikimedia", "openverse"] {
        mark_fetched(&store, IMAGES, word_id, source, 0).await;
    }

    let context = engine(fixture.dir.path(), SourcesConfig::default());
    assert!(derive(
        &store,
        Arc::new(GenImageSdxlRule::new(context)) as Arc<dyn Rule>
    )
    .await
    .is_empty());
}

// ---------------------------------------------------------------------------
// Examples
// ---------------------------------------------------------------------------

/// A fresh word is asked Tatoeba and nothing else: its Free Dictionary
/// sentences arrive with its definitions, in the same transaction, so no
/// separate job is ever needed for them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fresh_word_only_derives_the_tatoeba_example_job() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;

    let context = engine(fixture.dir.path(), SourcesConfig::default());
    let jobs = derive(
        &store,
        Arc::new(FetchExamplesRule::new(context)) as Arc<dyn Rule>,
    )
    .await;

    assert_eq!(
        job_subjects(&jobs),
        vec![format!("fetch_examples/{word_id}:tatoeba")]
    );
    assert_eq!(jobs[0].rate_key.to_string(), "tatoeba");
}

/// The backfill path: a word whose definitions were fetched by an older build
/// has a definitions marker and no examples marker, and only then is the
/// payload fetched a second time for its usage sentences.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_word_fetched_before_the_mining_gets_a_freedict_backfill() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;
    let context = engine(fixture.dir.path(), SourcesConfig::default());
    let rule = || Arc::new(FetchExamplesRule::new(context.clone())) as Arc<dyn Rule>;

    // Wave-3 shape: definitions answered, sentences never mined.
    mark_fetched(&store, DEFINITIONS, word_id, "freedict", 4).await;
    assert_eq!(
        job_subjects(&derive(&store, rule()).await),
        vec![
            format!("fetch_examples/{word_id}:freedict"),
            format!("fetch_examples/{word_id}:tatoeba"),
        ]
    );

    // Once the sentences have their own marker, the backfill stops. This is
    // also the wave-4 steady state, where both markers land together.
    mark_fetched(&store, "examples", word_id, "freedict", 2).await;
    assert_eq!(
        job_subjects(&derive(&store, rule()).await),
        vec![format!("fetch_examples/{word_id}:tatoeba")]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_exam_corpus_adds_a_third_example_source() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;

    let corpus = fixture.dir.path().join("corpus.jsonl");
    std::fs::write(
        &corpus,
        br#"{"word":"serene","sentence":"A serene lake lay below the ridge."}"#,
    )
    .unwrap();
    let context = engine(
        fixture.dir.path(),
        SourcesConfig {
            corpus_path: Some(corpus),
            ..SourcesConfig::default()
        },
    );

    assert_eq!(
        job_subjects(
            &derive(
                &store,
                Arc::new(FetchExamplesRule::new(context)) as Arc<dyn Rule>
            )
            .await
        ),
        vec![
            format!("fetch_examples/{word_id}:exam_corpus"),
            format!("fetch_examples/{word_id}:tatoeba"),
        ]
    );
}

/// An answered source is never asked twice, whatever it answered with.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answered_source_is_not_asked_again() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;
    for source in ["wikimedia", "openverse"] {
        mark_fetched(&store, IMAGES, word_id, source, 0).await;
    }
    mark_fetched(&store, "examples", word_id, "tatoeba", 3).await;

    let context = engine(fixture.dir.path(), SourcesConfig::default());
    assert!(derive(
        &store,
        Arc::new(FetchImagesRule::new(context.clone())) as Arc<dyn Rule>
    )
    .await
    .is_empty());
    assert!(derive(
        &store,
        Arc::new(FetchExamplesRule::new(context)) as Arc<dyn Rule>
    )
    .await
    .is_empty());
}
