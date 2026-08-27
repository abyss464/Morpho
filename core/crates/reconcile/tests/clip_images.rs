//! Semantic image selection: what the picture *depicts* decides the slot.
//!
//! Backlog #8 in one sentence — for two years the only things choosing between
//! a word's pictures were resolution and a part-of-speech hint, so the library
//! optimized for sharp pictures of the wrong thing, and the CLIP re-matching
//! that could have fixed it lived in an operator script the engine never saw.
//! These tests pin the three properties that make the fix safe to point at a
//! live database:
//!
//! * with scores present, semantics decides;
//! * with scores absent, *nothing whatsoever* changes;
//! * a picture a question mate already shows can never *win* a slot, so an
//!   automatic selection cannot create a collision the exporter's
//!   `question_images_distinct` gate would then fail on — and a slot that
//!   already holds one is replaced without having to clear the hysteresis
//!   margin, because an unanswerable card is not a matter of merit.

mod common;

use common::{
    converge, harness, run_to_quiescence, seed_clip_score, seed_distractors, seed_image, seed_word,
    selected_image_hash,
};
use morpho_domain::types::{ImageSource, MediaKind, Role};
use morpho_reconcile::score::{CLIP_CEIL, CLIP_FLOOR};
use morpho_store::MediaStore;

/// The content address of a byte string, read back out of the library the same
/// way the candidates were put into it.
fn hash_of(media: &MediaStore, bytes: &[u8]) -> String {
    media.put_bytes(bytes, MediaKind::Image).unwrap().file_hash
}

/// A sharp picture of the wrong thing against a soft picture of the right one.
///
/// This is the trade the old scorer got backwards on every abstract word in the
/// lexicon: "adapt" came back holding a crisp studio photograph of a power
/// adapter, and nothing in the engine could tell it apart from a picture of
/// somebody adapting to something.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_apt_picture_takes_the_slot_from_the_sharp_one() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let adapt = seed_word(&store, "adapt", Role::Target).await;
    seed_image(
        &store,
        &media,
        adapt,
        b"a power adapter",
        ImageSource::Unsplash,
    )
    .await;
    seed_image(
        &store,
        &media,
        adapt,
        b"someone adapting",
        ImageSource::Openverse,
    )
    .await;

    let wrong = hash_of(&media, b"a power adapter");
    let right = hash_of(&media, b"someone adapting");
    seed_clip_score(&store, adapt, &wrong, CLIP_FLOOR).await;
    seed_clip_score(&store, adapt, &right, CLIP_CEIL).await;

    converge(&common::local_reconciler(&fixture)).await;

    assert_eq!(
        selected_image_hash(&store, adapt).await.as_deref(),
        Some(right.as_str()),
        "the picture that matches the sentence must win the slot"
    );
}

/// The safety property the whole deployment rests on: a lexicon with no scores
/// at all behaves exactly as it did before any of this existed. A sidecar that
/// is down, unconfigured, or halfway through its backlog must not reorder a
/// single slot.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lexicon_with_no_scores_selects_exactly_as_it_did_before() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let serene = seed_word(&store, "serene", Role::Target).await;
    // Distinct quality: only the resolution prior separates them, and the
    // sharper one has the *higher* candidate id, so a tie-break could not
    // produce this answer by accident.
    seed_image(&store, &media, serene, b"soft", ImageSource::Wikimedia).await;
    seed_image(&store, &media, serene, b"sharp", ImageSource::Wikimedia).await;

    let reconciler = common::local_reconciler(&fixture);
    converge(&reconciler).await;
    let settled = selected_image_hash(&store, serene).await;
    assert!(settled.is_some());

    for _ in 0..3 {
        let stats = run_to_quiescence(&reconciler).await;
        assert_eq!(stats.sweep.selected, 0, "an unscored slot moved");
    }
    assert_eq!(settled, selected_image_hash(&store, serene).await);
}

/// A pool the sidecar has only half finished is ranked on quality alone.
///
/// The alternative would compare a scored candidate against an unscored one,
/// which is two different rulers — and a word mid-backfill would flip its slot
/// to whichever picture the sidecar happened to reach first, then flip back when
/// the rest arrived. So the semantic term is all or nothing, per word.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_half_scored_pool_waits_rather_than_ranking_on_two_rulers() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let lucid = seed_word(&store, "lucid", Role::Target).await;
    seed_image(&store, &media, lucid, b"lucid one", ImageSource::Wikimedia).await;
    seed_image(&store, &media, lucid, b"lucid two", ImageSource::Wikimedia).await;
    let first = hash_of(&media, b"lucid one");
    let second = hash_of(&media, b"lucid two");

    let reconciler = common::local_reconciler(&fixture);
    converge(&reconciler).await;
    let before = selected_image_hash(&store, lucid).await;

    // One candidate scored badly, the other not scored at all. Quality is equal,
    // so if the term were applied here the badly scored one would be pushed out
    // of the slot on the strength of half the evidence.
    seed_clip_score(&store, lucid, &before.clone().unwrap(), CLIP_FLOOR).await;
    converge(&reconciler).await;
    assert_eq!(
        selected_image_hash(&store, lucid).await,
        before,
        "half a pool's worth of evidence must not move a slot"
    );

    // Once the pool is scored uniformly, the better answer takes it.
    let other = if before.as_deref() == Some(first.as_str()) {
        second
    } else {
        first
    };
    seed_clip_score(&store, lucid, &other, CLIP_CEIL).await;
    converge(&reconciler).await;
    assert_eq!(
        selected_image_hash(&store, lucid).await.as_deref(),
        Some(other.as_str()),
        "a fully scored pool ranks on what the pictures mean"
    );
}

/// The export gate, made unreachable. Two words on one question card cannot
/// settle on one picture even when semantics prefers it for both — an option
/// grid showing the same image twice has no right answer, and that is not a
/// matter of merit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_question_mate_can_never_take_the_picture_its_neighbour_shows() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let contain = seed_word(&store, "contain", Role::Target).await;
    let container = seed_word(&store, "container", Role::Target).await;
    let filler_one = seed_word(&store, "content", Role::Target).await;
    let filler_two = seed_word(&store, "contest", Role::Target).await;
    seed_distractors(&store, contain, [container, filler_one, filler_two]).await;

    // Both hold the same crate photograph, and `container` also has one of its
    // own that CLIP likes rather less.
    seed_image(&store, &media, contain, b"a crate", ImageSource::Wikimedia).await;
    seed_image(
        &store,
        &media,
        container,
        b"a crate",
        ImageSource::Wikimedia,
    )
    .await;
    seed_image(
        &store,
        &media,
        container,
        b"a shipping yard",
        ImageSource::Wikimedia,
    )
    .await;

    let crate_hash = hash_of(&media, b"a crate");
    let yard = hash_of(&media, b"a shipping yard");
    // Semantics prefers the crate for both — which is exactly the situation the
    // veto has to survive, because the penalty alone would not.
    seed_clip_score(&store, contain, &crate_hash, CLIP_CEIL).await;
    seed_clip_score(&store, container, &crate_hash, CLIP_CEIL).await;
    seed_clip_score(&store, container, &yard, CLIP_FLOOR).await;

    converge(&common::local_reconciler(&fixture)).await;

    let first = selected_image_hash(&store, contain).await;
    let second = selected_image_hash(&store, container).await;
    assert!(first.is_some(), "contain still gets a picture");
    assert_ne!(
        first, second,
        "two words on one card must never show the same picture"
    );
}

/// The boundary of what the veto can do, stated so nobody has to rediscover it.
///
/// A word every one of whose pictures a mate also shows has nothing admissible
/// to move to. The reconciler never *clears* a selection — a slot only ever
/// changes to another candidate — so such a word keeps the picture it has, and
/// the collision surfaces at the export gate rather than being silently
/// resolved. That is the right division of labour: the word is already flagged
/// as needing more candidates (`needs_image_candidates` counts a picture
/// somebody else holds as no picture at all), so the image chain walks it down
/// to the second passes and then to generation, which is where a *new* picture
/// can actually come from.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cornered_word_keeps_its_picture_and_waits_for_a_new_one() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let ship = seed_word(&store, "ship", Role::Target).await;
    let vessel = seed_word(&store, "vessel", Role::Target).await;
    let filler_one = seed_word(&store, "shirt", Role::Target).await;
    let filler_two = seed_word(&store, "shin", Role::Target).await;
    seed_distractors(&store, vessel, [ship, filler_one, filler_two]).await;

    // `ship` gets the photograph first (lower word id, so it selects first);
    // `vessel` has nothing else.
    seed_image(&store, &media, ship, b"a boat", ImageSource::Wikimedia).await;
    seed_image(&store, &media, vessel, b"a boat", ImageSource::Wikimedia).await;

    let reconciler = common::local_reconciler(&fixture);
    converge(&reconciler).await;

    assert!(selected_image_hash(&store, ship).await.is_some());
    assert!(
        selected_image_hash(&store, vessel).await.is_some(),
        "a slot is never emptied; the collision is reported, not hidden"
    );
    // And it does not thrash: with nowhere admissible to go, both words hold
    // still rather than trading the picture back and forth every sixty seconds.
    for _ in 0..3 {
        let stats = run_to_quiescence(&reconciler).await;
        assert_eq!(stats.sweep.selected, 0, "a cornered slot moved");
    }
}

/// Scores settle. A lexicon that has been rescored reaches an answer and then
/// stops — which is what makes it safe to run the sweep every sixty seconds
/// forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_scored_lexicon_converges_and_stays_put() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let serene = seed_word(&store, "serene", Role::Target).await;
    let lucid = seed_word(&store, "lucid", Role::Target).await;
    for (word, bytes, score) in [
        (serene, &b"a still lake"[..], 0.29),
        (serene, &b"a lake at noon"[..], 0.21),
        (lucid, &b"clear water"[..], 0.27),
        (lucid, &b"a glass"[..], 0.12),
    ] {
        seed_image(&store, &media, word, bytes, ImageSource::Wikimedia).await;
        seed_clip_score(&store, word, &hash_of(&media, bytes), score).await;
    }

    let reconciler = common::local_reconciler(&fixture);
    converge(&reconciler).await;
    let settled = (
        selected_image_hash(&store, serene).await,
        selected_image_hash(&store, lucid).await,
    );
    assert_eq!(
        settled.0.as_deref(),
        Some(hash_of(&media, b"a still lake").as_str())
    );
    assert_eq!(
        settled.1.as_deref(),
        Some(hash_of(&media, b"clear water").as_str())
    );

    for _ in 0..3 {
        let stats = run_to_quiescence(&reconciler).await;
        assert_eq!(stats.sweep.selected, 0, "a settled scored slot moved");
    }
}
