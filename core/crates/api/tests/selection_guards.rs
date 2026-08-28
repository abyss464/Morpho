//! What the write path refuses, and what it must not.
//!
//! A slot pointing at a candidate that has left `status = 'available'` is the
//! defect the reconciler now heals. These are the two doors it came in through:
//! selecting a candidate nobody may show, and approving a slot that already
//! points at one — the second is how a bulk approval pass re-froze rows the
//! rejection had just released.
//!
//! The third test is the anomaly that was reported alongside them: a selection
//! POST over an existing pinned human row answering `200` while the row kept
//! its old candidate.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use morpho_api::{build_router, AppState};
use morpho_domain::event::Actor;
use morpho_domain::tts::TtsConfig;
use morpho_domain::types::{CandidateKind, CreatedBy, ExampleSource, Role, SelectedBy, SlotRef};
use morpho_export::ExportSettings;
use morpho_reconcile::JobRegistry;
use morpho_store::ops::{CreateWord, MintExampleCandidate};
use morpho_store::{Store, StoreConfig, WriteOp};

struct Harness {
    dir: tempfile::TempDir,
    store: Store,
    router: axum::Router,
}

impl Harness {
    /// Write straight to the file, around the store — the only way to state
    /// what a database already holds. The rows this reaches predate the guards
    /// above, which is exactly why no write op can produce them any more.
    fn force_sql(&self, sql: &str) {
        let conn = rusqlite::Connection::open(self.dir.path().join("working.db")).unwrap();
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        conn.execute_batch(sql).unwrap();
    }
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().to_path_buf();
    let store = Store::open(StoreConfig::new(data_dir.join("working.db"))).unwrap();
    let export = ExportSettings {
        tts: TtsConfig::default(),
        tokenizer_ver: "simple-tokenizer/1".to_string(),
        lemmatizer_ver: "lowercase-lemmatizer/1".to_string(),
        data_dir: data_dir.clone(),
        exporter: "morphod-test".to_string(),
    };
    let state = AppState::new(
        store.clone(),
        Arc::new(JobRegistry::new()),
        data_dir,
        export,
    );
    let router = build_router(state, None);
    Harness { dir, store, router }
}

async fn post(
    router: &axum::Router,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-morpho-user", "abyss")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn seed_word(store: &Store, lemma: &str) -> i64 {
    store
        .write(
            Actor::Cli,
            WriteOp::CreateWord(CreateWord::new(lemma, Role::Target, CreatedBy::Import)),
        )
        .await
        .unwrap()
        .result
        .word_id()
        .unwrap()
}

async fn seed_example(store: &Store, word_id: i64, text: &str, hl: (i64, i64)) -> i64 {
    store
        .write(
            Actor::Cli,
            WriteOp::MintExampleCandidate(MintExampleCandidate {
                word_id,
                text: text.to_string(),
                hl_start: hl.0,
                hl_end: hl.1,
                source: ExampleSource::Tatoeba,
                source_ref: None,
                created_by: None,
            }),
        )
        .await
        .unwrap()
        .result
        .cand_id()
        .unwrap()
}

/// The two sentences every test here works with, and where "serene" sits in
/// each.
async fn two_sentences(store: &Store, word_id: i64) -> (i64, i64) {
    let first = seed_example(store, word_id, "A serene lake lay below.", (2, 8)).await;
    let second = seed_example(store, word_id, "The serene monk answered.", (4, 10)).await;
    (first, second)
}

/// The candidate one example slot currently points at.
async fn slot_candidate(store: &Store, word_id: i64, slot: i64) -> Option<i64> {
    store
        .read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT ex_cand_id FROM example_selections WHERE word_id = ?1 AND slot = ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![word_id, slot], |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows.into_iter().next())
        })
        .await
        .unwrap()
}

async fn approved(store: &Store, word_id: i64, slot: i64) -> bool {
    store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT approved FROM example_selections WHERE word_id = ?1 AND slot = ?2",
                rusqlite::params![word_id, slot],
                |row| row.get::<_, i64>(0),
            )? != 0)
        })
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------
// The two guards
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn selecting_a_rejected_candidate_is_a_conflict() {
    let h = harness();
    let word_id = seed_word(&h.store, "serene").await;
    let (first, second) = two_sentences(&h.store, word_id).await;

    h.store
        .write(
            Actor::admin("abyss"),
            WriteOp::RejectCandidate {
                kind: CandidateKind::Example,
                cand_id: second,
            },
        )
        .await
        .unwrap();

    let (status, body) = post(
        &h.router,
        "/api/selections/example",
        serde_json::json!({ "word_id": word_id, "slot": 1, "cand_id": second }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "conflict");
    assert_eq!(slot_candidate(&h.store, word_id, 1).await, None);

    // The available sentence still goes in, so the guard is about the status
    // and nothing else.
    let (status, _) = post(
        &h.router,
        "/api/selections/example",
        serde_json::json!({ "word_id": word_id, "slot": 1, "cand_id": first }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(slot_candidate(&h.store, word_id, 1).await, Some(first));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn approving_a_slot_on_a_rejected_candidate_is_a_conflict() {
    let h = harness();
    let word_id = seed_word(&h.store, "serene").await;
    let (first, _) = two_sentences(&h.store, word_id).await;
    let slot = SlotRef::Example { word_id, slot: 1 };

    h.store
        .write(
            Actor::admin("abyss"),
            WriteOp::select(slot.clone(), first, SelectedBy::Human),
        )
        .await
        .unwrap();

    // Rejecting the selected sentence releases the slot but leaves it pointing
    // there — the reconciler re-selects, the write path never empties a slot.
    h.store
        .write(
            Actor::admin("abyss"),
            WriteOp::RejectCandidate {
                kind: CandidateKind::Example,
                cand_id: first,
            },
        )
        .await
        .unwrap();

    let (status, body) = post(
        &h.router,
        "/api/selections/example/approve",
        serde_json::json!({ "word_id": word_id, "slot": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "conflict");
    assert!(
        !approved(&h.store, word_id, 1).await,
        "the approval that would re-freeze the slot must not land"
    );
}

// ---------------------------------------------------------------------------
// The reported anomaly
// ---------------------------------------------------------------------------

/// A human override lands on a slot another human already pinned.
///
/// This is the case reported as answering `200` while the row kept its old
/// candidate. It does not: the switch applies, `selection_rev` moves and the
/// approval that was riding on the old content is invalidated. Pinning protects
/// a slot from *automatic* selection (README rule 4), never from an editor.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_human_override_moves_a_pinned_approved_slot() {
    let h = harness();
    let word_id = seed_word(&h.store, "serene").await;
    let (first, second) = two_sentences(&h.store, word_id).await;
    let slot = SlotRef::Example { word_id, slot: 1 };

    h.store
        .write(
            Actor::admin("abyss"),
            WriteOp::select(slot.clone(), first, SelectedBy::Human),
        )
        .await
        .unwrap();
    h.store
        .write(Actor::admin("abyss"), WriteOp::approve(slot))
        .await
        .unwrap();

    let (status, body) = post(
        &h.router,
        "/api/selections/example",
        serde_json::json!({ "word_id": word_id, "slot": 1, "cand_id": second }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        slot_candidate(&h.store, word_id, 1).await,
        Some(second),
        "a 200 has to mean the slot moved"
    );
    assert!(!approved(&h.store, word_id, 1).await);
    assert_eq!(
        body["examples"][0]["selection"]["ex_cand_id"],
        serde_json::json!(second),
        "and the answer the console reads back has to say so too"
    );
}

/// The same override, on a row already in the corrupt state: pinned, approved,
/// and pointing at a rejected sentence. This is the escape hatch an editor
/// needs, and it has to work whichever order the repair happens in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_override_rescues_a_slot_stuck_on_a_rejected_sentence() {
    let h = harness();
    let word_id = seed_word(&h.store, "serene").await;
    let (first, second) = two_sentences(&h.store, word_id).await;
    let slot = SlotRef::Example { word_id, slot: 1 };

    h.store
        .write(
            Actor::admin("abyss"),
            WriteOp::select(slot.clone(), first, SelectedBy::Human),
        )
        .await
        .unwrap();
    h.store
        .write(Actor::admin("abyss"), WriteOp::approve(slot))
        .await
        .unwrap();
    h.force_sql(&format!(
        "UPDATE example_candidates SET status = 'rejected' WHERE ex_cand_id = {first}"
    ));

    let (status, body) = post(
        &h.router,
        "/api/selections/example",
        serde_json::json!({ "word_id": word_id, "slot": 1, "cand_id": second }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(slot_candidate(&h.store, word_id, 1).await, Some(second));
    assert!(!approved(&h.store, word_id, 1).await);
}

/// The one way a selection POST legitimately leaves the row where it was: the
/// sentence asked for is already in one of the word's other slots.
/// `UNIQUE (word_id, ex_cand_id)` forbids showing it twice, and silently
/// emptying the other slot is not what the caller asked for — so it is refused
/// rather than quietly ignored.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sentence_the_word_already_shows_is_refused_not_ignored() {
    let h = harness();
    let word_id = seed_word(&h.store, "serene").await;
    let (first, second) = two_sentences(&h.store, word_id).await;

    for (slot, cand) in [(1, first), (2, second)] {
        let (status, _) = post(
            &h.router,
            "/api/selections/example",
            serde_json::json!({ "word_id": word_id, "slot": slot, "cand_id": cand }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let (status, body) = post(
        &h.router,
        "/api/selections/example",
        serde_json::json!({ "word_id": word_id, "slot": 1, "cand_id": second }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(slot_candidate(&h.store, word_id, 1).await, Some(first));
    assert_eq!(slot_candidate(&h.store, word_id, 2).await, Some(second));
}

// ---------------------------------------------------------------------------
// Purge
// ---------------------------------------------------------------------------

async fn delete(router: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("x-morpho-user", "abyss")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn candidate_exists(store: &Store, cand_id: i64) -> bool {
    store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM example_candidates WHERE ex_cand_id = ?1)",
                rusqlite::params![cand_id],
                |row| row.get::<_, i64>(0),
            )? != 0)
        })
        .await
        .unwrap()
}

/// Erasure is the administrator's, and it is the only thing that removes a
/// candidate row: nothing else in the engine ever deletes one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn purging_an_unselected_sentence_removes_the_row_and_records_it() {
    let h = harness();
    let word_id = seed_word(&h.store, "serene").await;
    let (first, second) = two_sentences(&h.store, word_id).await;

    let (status, body) = post(
        &h.router,
        "/api/selections/example",
        serde_json::json!({ "word_id": word_id, "slot": 1, "cand_id": first }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = delete(&h.router, &format!("/api/candidates/example/{second}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!candidate_exists(&h.store, second).await);
    assert!(candidate_exists(&h.store, first).await);

    // The row is gone; the audit says what it was.
    let purged = h
        .store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT detail FROM events
                  WHERE entity_type = 'example_candidate' AND entity_id = ?1
                    AND action = 'candidate_purged'",
                rusqlite::params![second.to_string()],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap();
    let detail: serde_json::Value = serde_json::from_str(&purged).unwrap();
    assert_eq!(detail["word_id"], serde_json::json!(word_id));
    assert!(detail["text_hash"].as_str().is_some_and(|h| !h.is_empty()));

    // And it is gone for good.
    let (status, _) = delete(&h.router, &format!("/api/candidates/example/{second}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A sentence a slot still shows is refused rather than deleted, and the
/// message says which slot to move first.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn purging_a_selected_sentence_is_a_conflict() {
    let h = harness();
    let word_id = seed_word(&h.store, "serene").await;
    let (first, _) = two_sentences(&h.store, word_id).await;
    h.store
        .write(
            Actor::admin("abyss"),
            WriteOp::select(
                SlotRef::Example { word_id, slot: 2 },
                first,
                SelectedBy::Human,
            ),
        )
        .await
        .unwrap();

    let (status, body) = delete(&h.router, &format!("/api/candidates/example/{first}")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "conflict");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("slot 2"));
    assert!(candidate_exists(&h.store, first).await);
    assert_eq!(slot_candidate(&h.store, word_id, 2).await, Some(first));
}

/// Re-posting the sentence a slot already holds is a flag update, and the
/// approval on that same content survives it — the triple (slot, candidate,
/// content) has not changed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn re_selecting_the_same_sentence_only_moves_the_flags() {
    let h = harness();
    let word_id = seed_word(&h.store, "serene").await;
    let (first, _) = two_sentences(&h.store, word_id).await;
    let slot = SlotRef::Example { word_id, slot: 1 };

    h.store
        .write(
            Actor::admin("abyss"),
            WriteOp::select(slot.clone(), first, SelectedBy::Human),
        )
        .await
        .unwrap();
    h.store
        .write(Actor::admin("abyss"), WriteOp::approve(slot))
        .await
        .unwrap();

    let (status, _) = post(
        &h.router,
        "/api/selections/example",
        serde_json::json!({ "word_id": word_id, "slot": 1, "cand_id": first, "pin": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(slot_candidate(&h.store, word_id, 1).await, Some(first));
    assert!(
        approved(&h.store, word_id, 1).await,
        "same slot, same candidate, same content: nothing invalidates the approval"
    );
}
