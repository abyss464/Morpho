//! Automatic selection of the one image slot, and the global-uniqueness
//! pressure on it.
//!
//! Media is content addressed, which is what makes the library cheap and what
//! makes this necessary: two words that mean nearly the same thing search for
//! nearly the same thing and come back holding one identical `file_hash`. Every
//! step of that is correct in isolation and wrong at the card, where a question
//! renders the word beside its three fixed distractors and two identical option
//! images leave the learner nothing to choose between.
//!
//! So a candidate another word already shows is ranked below one nobody does,
//! by a margin wide enough to actually move a slot. These tests pin that the
//! pressure resolves a collision, that it converges, and — the part that matters
//! on a live database — that it does nothing whatsoever to a pool with no shared
//! hash in it.

mod common;

use common::{converge, harness, seed_image, seed_word, selected_image_hash};
use morpho_domain::types::{ImageSource, MediaKind, Role};
use morpho_store::MediaStore;

/// One byte string reaching two words: exactly what the stock libraries do to
/// `adapt` and `adapter`.
const SHARED: &[u8] = b"one webp, two words";
/// A picture only one of them has.
const ALTERNATIVE: &[u8] = b"a second webp entirely";

/// The content address of a byte string, read back out of the library the same
/// way the candidates were put into it.
fn hash_of(media: &MediaStore, bytes: &[u8]) -> String {
    media.put_bytes(bytes, MediaKind::Image).unwrap().file_hash
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_words_never_settle_on_the_same_picture() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let adapt = seed_word(&store, "adapt", Role::Target).await;
    let adapter = seed_word(&store, "adapter", Role::Target).await;

    // `adapt` has nothing but the shared picture. `adapter` has it too — it is
    // even the lower candidate id, so the first pass hands it to both — plus one
    // of its own.
    seed_image(&store, &media, adapt, SHARED, ImageSource::Wikimedia).await;
    seed_image(&store, &media, adapter, SHARED, ImageSource::Wikimedia).await;
    seed_image(&store, &media, adapter, ALTERNATIVE, ImageSource::Wikimedia).await;

    converge(&common::local_reconciler(&fixture)).await;

    let first = selected_image_hash(&store, adapt).await.expect("adapt");
    let second = selected_image_hash(&store, adapter).await.expect("adapter");
    assert_ne!(
        first, second,
        "the sweep must not leave two words showing one picture"
    );
}

/// The scarce picture goes to the word that has no alternative, because the word
/// that does has somewhere to move and the other has not.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_word_with_a_choice_is_the_one_that_moves() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let adapt = seed_word(&store, "adapt", Role::Target).await;
    let adapter = seed_word(&store, "adapter", Role::Target).await;
    seed_image(&store, &media, adapt, SHARED, ImageSource::Wikimedia).await;
    seed_image(&store, &media, adapter, SHARED, ImageSource::Wikimedia).await;
    seed_image(&store, &media, adapter, ALTERNATIVE, ImageSource::Wikimedia).await;

    converge(&common::local_reconciler(&fixture)).await;

    assert_eq!(
        selected_image_hash(&store, adapt).await.as_deref(),
        Some(hash_of(&media, SHARED).as_str()),
        "the word with no alternative keeps the contested picture"
    );
    assert_eq!(
        selected_image_hash(&store, adapter).await.as_deref(),
        Some(hash_of(&media, ALTERNATIVE).as_str()),
        "the word with somewhere to go is the one that goes"
    );
}

/// A word whose only candidate is somebody else's picture keeps it. The pressure
/// prefers a different picture; it never prefers no picture, and the second-pass
/// chain is what actually goes looking for one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cornered_word_keeps_the_duplicate_rather_than_emptying_its_slot() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let adapt = seed_word(&store, "adapt", Role::Target).await;
    let adapter = seed_word(&store, "adapter", Role::Target).await;
    seed_image(&store, &media, adapt, SHARED, ImageSource::Wikimedia).await;
    seed_image(&store, &media, adapter, SHARED, ImageSource::Wikimedia).await;

    converge(&common::local_reconciler(&fixture)).await;

    assert!(selected_image_hash(&store, adapt).await.is_some());
    assert!(selected_image_hash(&store, adapter).await.is_some());
}

/// The guarantee that made this safe to ship onto a live database: with no
/// shared hash anywhere, every score and every slot is exactly what it was. The
/// selection is reached on the first pass and never moves again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lexicon_with_no_shared_picture_never_moves() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());

    let serene = seed_word(&store, "serene", Role::Target).await;
    let lucid = seed_word(&store, "lucid", Role::Target).await;
    // Two candidates each, all four distinct, and the better source second so
    // the ranking has real work to do.
    seed_image(
        &store,
        &media,
        serene,
        b"serene one",
        ImageSource::Wikimedia,
    )
    .await;
    seed_image(&store, &media, serene, b"serene two", ImageSource::Unsplash).await;
    seed_image(&store, &media, lucid, b"lucid one", ImageSource::Wikimedia).await;
    seed_image(&store, &media, lucid, b"lucid two", ImageSource::Unsplash).await;

    let reconciler = common::local_reconciler(&fixture);
    converge(&reconciler).await;

    let settled = (
        selected_image_hash(&store, serene).await,
        selected_image_hash(&store, lucid).await,
    );
    assert!(settled.0.is_some() && settled.1.is_some());
    assert_ne!(settled.0, settled.1);

    // Three more sweeps over unchanged state change nothing.
    for _ in 0..3 {
        let stats = common::run_to_quiescence(&reconciler).await;
        assert_eq!(stats.sweep.selected, 0, "a settled slot moved");
    }
    assert_eq!(
        settled,
        (
            selected_image_hash(&store, serene).await,
            selected_image_hash(&store, lucid).await,
        )
    );
}
