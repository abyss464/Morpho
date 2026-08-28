//! README rule 4's exception, applied to slots that are already wrong.
//!
//! A pin protects a slot from automatic selection, and an approval implies a
//! pin. Neither is supposed to survive the candidate underneath it leaving
//! `status = 'available'` — but a live database was found holding five example
//! and sixty-four image selections that were pinned, approved and pointing at
//! rejected candidates, because rejecting a candidate released the slot while
//! a later bulk approval simply froze it again.
//!
//! So the release cannot only be a reaction to a rejection. The sweep has to
//! reach the rows that are already broken: an occupant nobody may show is not
//! an incumbent to be out-argued, the pin and the approval come off, and the
//! best available candidate takes the slot outright. When there is no available
//! candidate the row keeps pointing where it points — the reconciler never
//! empties a slot — but the pin and the approval still come off, so readiness
//! and the export gate report the word honestly.

mod common;

use common::{converge, harness, seed_example, seed_image, seed_word, Harness};
use morpho_domain::event::Actor;
use morpho_domain::types::{ExampleSource, ImageSource, Role, SelectedBy, SlotRef};
use morpho_store::{MediaStore, Store, WriteOp};

/// `(ex_cand_id, pinned, approved)` of one example slot.
async fn example_slot(store: &Store, word_id: i64, slot: i64) -> Option<(i64, bool, bool)> {
    store
        .read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT ex_cand_id, pinned, approved FROM example_selections
                 WHERE word_id = ?1 AND slot = ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![word_id, slot], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)? != 0,
                        row.get::<_, i64>(2)? != 0,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows.into_iter().next())
        })
        .await
        .unwrap()
}

/// `(img_cand_id, pinned, approved)` of a word's one image slot.
async fn image_slot(store: &Store, word_id: i64) -> Option<(i64, bool, bool)> {
    store
        .read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT img_cand_id, pinned, approved FROM image_selections WHERE word_id = ?1",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![word_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)? != 0,
                        row.get::<_, i64>(2)? != 0,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows.into_iter().next())
        })
        .await
        .unwrap()
}

/// How many audit rows of one action name one slot.
async fn event_count(store: &Store, entity_type: &str, entity_id: &str, action: &str) -> i64 {
    let (entity_type, entity_id, action) = (
        entity_type.to_string(),
        entity_id.to_string(),
        action.to_string(),
    );
    store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM events
                 WHERE entity_type = ?1 AND entity_id = ?2 AND action = ?3",
                rusqlite::params![entity_type, entity_id, action],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .await
        .unwrap()
}

/// Hand one slot to a human and approve it, the way the console does.
async fn claim_and_approve(store: &Store, slot: SlotRef, cand_id: i64) {
    store
        .write(
            Actor::admin("abyss"),
            WriteOp::select(slot.clone(), cand_id, SelectedBy::Human),
        )
        .await
        .unwrap();
    store
        .write(Actor::admin("abyss"), WriteOp::approve(slot))
        .await
        .unwrap();
}

/// Reject a candidate behind the store's back, leaving the selection pinned and
/// approved on top of it. This is the shape the live rows were found in.
fn corrupt(fixture: &Harness, table: &str, pk: &str, cand_id: i64) {
    fixture.force_sql(&format!(
        "UPDATE {table} SET status = 'rejected' WHERE {pk} = {cand_id}"
    ));
}

// ---------------------------------------------------------------------------
// Examples
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pinned_approved_example_on_a_rejected_candidate_falls_back() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;

    let first = seed_example(
        &store,
        word_id,
        "serene",
        "A serene lake lay below the ridge.",
        ExampleSource::ExamCorpus,
    )
    .await;
    let second = seed_example(
        &store,
        word_id,
        "serene",
        "The serene monk answered every question.",
        ExampleSource::Tatoeba,
    )
    .await;

    let slot_one = SlotRef::Example { word_id, slot: 1 };
    claim_and_approve(&store, slot_one.clone(), first).await;
    assert_eq!(
        example_slot(&store, word_id, 1).await,
        Some((first, true, true))
    );

    corrupt(&fixture, "example_candidates", "ex_cand_id", first);
    converge(&common::local_reconciler(&fixture)).await;

    assert_eq!(
        example_slot(&store, word_id, 1).await,
        Some((second, false, false)),
        "the slot takes the one sentence still available, unpinned and unapproved"
    );
    let entity_id = word_id.to_string() + ":1";
    assert_eq!(
        event_count(&store, "example_selection", &entity_id, "pin_fallback").await,
        1,
        "the release is recorded once, not once per pass"
    );
    assert_eq!(
        event_count(
            &store,
            "example_selection",
            &entity_id,
            "approval_invalidated"
        )
        .await,
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_example_with_no_replacement_is_released_where_it_stands() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;
    let only = seed_example(
        &store,
        word_id,
        "serene",
        "A serene lake lay below the ridge.",
        ExampleSource::ExamCorpus,
    )
    .await;

    claim_and_approve(&store, SlotRef::Example { word_id, slot: 1 }, only).await;
    corrupt(&fixture, "example_candidates", "ex_cand_id", only);
    converge(&common::local_reconciler(&fixture)).await;

    assert_eq!(
        example_slot(&store, word_id, 1).await,
        Some((only, false, false)),
        "the reconciler never empties a slot; it only lets go of it"
    );
    let entity_id = word_id.to_string() + ":1";
    assert_eq!(
        event_count(&store, "example_selection", &entity_id, "pin_fallback").await,
        1,
        "a word with nothing to fall back on must not log a release every pass"
    );
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pinned_approved_image_on_a_rejected_candidate_falls_back() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());
    let word_id = seed_word(&store, "serene", Role::Target).await;

    let first = seed_image(&store, &media, word_id, b"one webp", ImageSource::Wikimedia).await;
    let second = seed_image(
        &store,
        &media,
        word_id,
        b"another webp",
        ImageSource::Wikimedia,
    )
    .await;

    claim_and_approve(&store, SlotRef::Image { word_id }, first).await;
    assert_eq!(image_slot(&store, word_id).await, Some((first, true, true)));

    corrupt(&fixture, "image_candidates", "img_cand_id", first);
    converge(&common::local_reconciler(&fixture)).await;

    assert_eq!(
        image_slot(&store, word_id).await,
        Some((second, false, false)),
        "the picture still available takes the slot outright"
    );
    let entity_id = word_id.to_string();
    assert_eq!(
        event_count(&store, "image_selection", &entity_id, "pin_fallback").await,
        1
    );
    assert_eq!(
        event_count(
            &store,
            "image_selection",
            &entity_id,
            "approval_invalidated"
        )
        .await,
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_image_with_no_replacement_is_released_where_it_stands() {
    let fixture = harness();
    let store = fixture.store.clone();
    let media = MediaStore::new(fixture.dir.path());
    let word_id = seed_word(&store, "serene", Role::Target).await;
    let only = seed_image(&store, &media, word_id, b"one webp", ImageSource::Wikimedia).await;

    claim_and_approve(&store, SlotRef::Image { word_id }, only).await;
    corrupt(&fixture, "image_candidates", "img_cand_id", only);
    converge(&common::local_reconciler(&fixture)).await;

    assert_eq!(
        image_slot(&store, word_id).await,
        Some((only, false, false)),
        "the word keeps its picture and loses the protection it should not have"
    );
    assert_eq!(
        event_count(
            &store,
            "image_selection",
            &word_id.to_string(),
            "pin_fallback"
        )
        .await,
        1
    );
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pinned_approved_definition_on_a_rejected_candidate_falls_back() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;

    let first = common::seed_definition(&store, word_id, "adj", "calm and untroubled").await;
    let second = common::seed_definition(&store, word_id, "adj", "peaceful and still").await;

    let slot = SlotRef::Definition {
        word_id,
        pos: "adj".to_string(),
    };
    claim_and_approve(&store, slot, first).await;

    corrupt(&fixture, "definition_candidates", "def_cand_id", first);
    converge(&common::local_reconciler(&fixture)).await;

    let held = store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT def_cand_id, pinned, approved FROM definition_selections
                 WHERE word_id = ?1 AND pos = 'adj'",
                rusqlite::params![word_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)? != 0,
                        row.get::<_, i64>(2)? != 0,
                    ))
                },
            )?)
        })
        .await
        .unwrap();
    assert_eq!(held, (second, false, false));
}
