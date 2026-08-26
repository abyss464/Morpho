//! Automatic selection of the three example slots.
//!
//! A word needs slot 1 to be shippable, but the app's review mode reads all
//! three, so a selector that only ever fills slot 1 would look correct in the
//! readiness report and be wrong in the product. These tests pin the whole
//! ladder: distinct candidates fill distinct slots, in score order, without
//! ever pointing two slots at the same sentence.

mod common;

use common::{converge, harness, seed_example, seed_word};
use morpho_domain::event::Actor;
use morpho_domain::types::{ExampleSource, Role, SelectedBy, SlotRef};
use morpho_store::{Store, WriteOp};

/// `(slot, ex_cand_id, selected_by, auto_score)` for one word, in slot order.
async fn slots(store: &Store, word_id: i64) -> Vec<(i64, i64, String, f64)> {
    store
        .read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT es.slot, es.ex_cand_id, es.selected_by, COALESCE(ec.auto_score, 0.0)
                 FROM example_selections es
                 JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
                 WHERE es.word_id = ?1 ORDER BY es.slot",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![word_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, f64>(3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auto_selection_fills_all_three_slots() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "serene", Role::Target).await;

    for text in [
        "A serene lake lay below the ridge.",
        "The serene monk answered every question.",
        "Her serene expression never once changed.",
        "A serene morning followed the storm.",
    ] {
        seed_example(&store, word_id, "serene", text, ExampleSource::Tatoeba).await;
    }

    let reconciler = common::local_reconciler(&fixture);
    converge(&reconciler).await;

    let filled = slots(&store, word_id).await;
    assert_eq!(
        filled.iter().map(|(slot, ..)| *slot).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "every slot a distinct candidate could fill must be filled"
    );

    // Three distinct sentences, never the same one twice.
    let chosen: std::collections::HashSet<i64> =
        filled.iter().map(|(_, cand_id, ..)| *cand_id).collect();
    assert_eq!(chosen.len(), 3, "{filled:?}");

    // Best first: slot 1 is the mode-1 sentence and gets the top score.
    let scores: Vec<f64> = filled.iter().map(|(.., score)| *score).collect();
    assert!(
        scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "slots must fill in score order, got {scores:?}"
    );
    assert!(
        filled.iter().all(|(_, _, by, _)| by == "auto"),
        "nothing here was chosen by a human"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fewer_candidates_than_slots_fills_what_it_can() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "adapt", Role::Target).await;
    seed_example(
        &store,
        word_id,
        "adapt",
        "Species adapted to a warming climate.",
        ExampleSource::Freedict,
    )
    .await;
    seed_example(
        &store,
        word_id,
        "adapt",
        "Good teams adapt when the plan fails.",
        ExampleSource::Tatoeba,
    )
    .await;

    let reconciler = common::local_reconciler(&fixture);
    converge(&reconciler).await;

    let filled = slots(&store, word_id).await;
    assert_eq!(
        filled.iter().map(|(slot, ..)| *slot).collect::<Vec<_>>(),
        vec![1, 2],
        "two candidates fill two slots and leave the third empty"
    );
    // Ruling #18's prior in action: the dictionary's own usage line outranks a
    // sentence that merely happens to contain the word.
    let (_, top, _, _) = filled[0];
    let source: String = store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT source FROM example_candidates WHERE ex_cand_id = ?1",
                rusqlite::params![top],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(source, "freedict");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pinned_slot_keeps_its_sentence_and_the_rest_fill_around_it() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "lucid", Role::Target).await;

    let mut candidates = Vec::new();
    for text in [
        "A lucid explanation settled the argument.",
        "The patient was lucid throughout the night.",
        "Her lucid prose carried the whole chapter.",
    ] {
        candidates.push(seed_example(&store, word_id, "lucid", text, ExampleSource::Tatoeba).await);
    }

    // A human pins the last candidate into slot 1.
    store
        .write(
            Actor::Admin("tester".into()),
            WriteOp::select(
                SlotRef::Example { word_id, slot: 1 },
                candidates[2],
                SelectedBy::Human,
            ),
        )
        .await
        .unwrap();

    let reconciler = common::local_reconciler(&fixture);
    converge(&reconciler).await;

    let filled = slots(&store, word_id).await;
    assert_eq!(filled.len(), 3, "{filled:?}");
    assert_eq!(filled[0].1, candidates[2], "the pin is untouchable");
    assert_eq!(filled[0].2, "human");
    // And the pinned sentence is never handed to a second slot as well.
    assert!(
        filled[1..]
            .iter()
            .all(|(_, cand_id, ..)| *cand_id != candidates[2]),
        "{filled:?}"
    );
}
