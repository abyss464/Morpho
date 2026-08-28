//! Admin API integration tests: drive the real router with a real temp
//! database, no network.
//!
//! Every assertion here is a claim about the wire shape in
//! `admin-ui/src/api/types.ts`, because that file is the normative reference
//! (admin-api.md wave-2 ruling #1) and drift is invisible until the console
//! breaks.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use morpho_api::{build_router, AppState};
use morpho_domain::event::Actor;
use morpho_domain::tts::TtsConfig;
use morpho_domain::types::{
    CreatedBy, DefinitionSource, ExampleSource, ImageSource, MediaKind, Role, SelectedBy, SlotRef,
};
use morpho_export::ExportSettings;
use morpho_reconcile::JobRegistry;
use morpho_store::ops::{
    ApplyClipScores, ApplyReadiness, BindDistractors, ClipScoreRow, CreateWord, DistractorBinding,
    MediaRegistration, MintExampleCandidate, MintImageCandidate, ReadinessRow,
};
use morpho_store::{Store, StoreConfig, WriteOp};

struct Harness {
    _dir: tempfile::TempDir,
    data_dir: std::path::PathBuf,
    store: Store,
    router: axum::Router,
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
        data_dir.clone(),
        export,
    );
    let router = build_router(state, None);
    Harness {
        _dir: dir,
        data_dir,
        store,
        router,
    }
}

async fn call(router: &axum::Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

async fn get(router: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    call(
        router,
        Request::builder().uri(uri).body(Body::empty()).unwrap(),
    )
    .await
}

async fn post(
    router: &axum::Router,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    call(
        router,
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-morpho-user", "abyss")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

async fn delete(
    router: &axum::Router,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    call(
        router,
        Request::builder()
            .method("DELETE")
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-morpho-user", "abyss")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

async fn seed_word(store: &Store, lemma: &str, role: Role, rank: Option<i64>) -> i64 {
    let mut req = CreateWord::new(lemma, role, CreatedBy::Import);
    req.frequency_rank = rank;
    store
        .write(Actor::Cli, WriteOp::CreateWord(req))
        .await
        .unwrap()
        .result
        .word_id()
        .unwrap()
}

async fn seed_definition(store: &Store, word_id: i64, pos: &str, text: &str) -> i64 {
    store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(word_id, pos, text, DefinitionSource::Freedict),
        )
        .await
        .unwrap()
        .result
        .def_cand_id()
        .unwrap()
}

async fn select_definition(store: &Store, word_id: i64, pos: &str, cand: i64) {
    store
        .write(
            Actor::Reconciler,
            WriteOp::select(
                SlotRef::Definition {
                    word_id,
                    pos: pos.to_string(),
                },
                cand,
                SelectedBy::Auto,
            ),
        )
        .await
        .unwrap();
}

/// A word with one selected definition, ready for detail assertions.
async fn seeded_word(h: &Harness) -> i64 {
    let word = seed_word(&h.store, "benevolent", Role::Target, Some(4312)).await;
    seed_word(&h.store, "kind", Role::Base, None).await;
    let cand = seed_definition(&h.store, word, "adj", "well meaning and kind").await;
    select_definition(&h.store, word, "adj", cand).await;
    word
}

// ---------------------------------------------------------------------------
// Dashboard, events, jobs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dashboard_matches_the_wire_shape() {
    let h = harness();
    seeded_word(&h).await;

    let (status, body) = get(&h.router, "/api/dashboard").await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["words"]["total"], 2);
    assert_eq!(body["words"]["target"], 1);
    // Ruling #8: active auxiliaries only, and there are none.
    assert_eq!(body["words"]["auxiliary"], 0);
    assert_eq!(body["words"]["ready"], 0);
    assert_eq!(body["words"]["blocked"], 1);

    // Every asset bucket is an AssetRollup, not a bare count.
    for asset in ["definitions", "examples", "images", "tts"] {
        let rollup = &body["assets"][asset];
        assert!(rollup["ready"].is_number(), "{asset}: {rollup}");
        assert!(rollup["missing"].is_number(), "{asset}: {rollup}");
        assert!(rollup["failed"].is_number(), "{asset}: {rollup}");
    }
    // The word and its definition both need speaking.
    assert_eq!(body["assets"]["tts"]["missing"], 2);

    assert_eq!(body["oos_open"], 0);
    assert_eq!(body["dead_letters"], 0);
    assert!(body["plan"].is_null());
    assert!(body["recent_events"].is_array());
}

#[tokio::test]
async fn events_are_paginated_with_decoded_detail() {
    let h = harness();
    seeded_word(&h).await;

    let (status, body) = get(&h.router, "/api/events?page=1&page_size=2").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["total"].as_i64().unwrap() >= 2);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    // `detail` is a TEXT column holding JSON; the API decodes it.
    assert!(body["items"][0]["detail"].is_object());
    assert!(body["items"][0]["event_id"].is_number());
}

#[tokio::test]
async fn jobs_snapshot_matches_the_wire_shape() {
    let h = harness();
    let (status, body) = get(&h.router, "/api/jobs").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["in_flight"].is_array());
    assert!(body["backoff"].is_array());
    assert!(body["lanes"].is_object());
}

#[tokio::test]
async fn backoff_rows_carry_their_subject_label() {
    use morpho_domain::job::{JobKey, JobKind, JobStatus, RateKey, SubjectRef};
    use morpho_store::ops::UpsertJobState;

    let h = harness();
    let word = seed_word(&h.store, "serene", Role::Target, Some(4602)).await;
    h.store
        .write(
            Actor::Reconciler,
            WriteOp::UpsertJobState(UpsertJobState {
                key: JobKey::new(
                    JobKind::FetchImages,
                    SubjectRef::word_source(word, "unsplash"),
                ),
                rate_key: RateKey::Unsplash,
                status: JobStatus::Backoff,
                attempts: 2,
                next_retry_at: Some("2026-08-26T12:00:00.000Z".into()),
                last_error: Some("connection reset".into()),
            }),
        )
        .await
        .unwrap();

    let (_, body) = get(&h.router, "/api/jobs").await;
    let row = &body["backoff"][0];
    assert_eq!(row["kind"], "fetch_images");
    assert_eq!(row["status"], "backoff");
    assert_eq!(row["attempts"], 2);
    assert_eq!(row["subject_label"], "serene (unsplash)");
    assert_eq!(row["last_error"], "connection reset");
}

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

#[tokio::test]
async fn word_list_rollups_use_real_booleans() {
    let h = harness();
    seeded_word(&h).await;

    let (status, body) = get(&h.router, "/api/words?role=target").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 1);
    let item = &body["items"][0];
    assert_eq!(item["lemma"], "benevolent");
    assert_eq!(item["ready"], false, "a JSON boolean, not 0");
    assert!(item["ready"].is_boolean());
    assert!(item["has_image"].is_boolean());
    assert_eq!(item["sense_count"], 1);
    assert_eq!(item["example_count"], 0);
    assert_eq!(item["tts_missing"], 2);
    assert!(item["blockers"].is_array());
}

#[tokio::test]
async fn word_list_filters_apply() {
    let h = harness();
    seeded_word(&h).await;
    seed_word(&h.store, "serene", Role::Target, Some(4602)).await;

    let (_, all) = get(&h.router, "/api/words").await;
    assert_eq!(all["total"], 3);
    let (_, targets) = get(&h.router, "/api/words?role=target").await;
    assert_eq!(targets["total"], 2);
    let (_, searched) = get(&h.router, "/api/words?q=sere").await;
    assert_eq!(searched["total"], 1);
    let (_, ready) = get(&h.router, "/api/words?ready=true").await;
    assert_eq!(ready["total"], 0);
}

#[tokio::test]
async fn word_detail_matches_the_wire_shape() {
    let h = harness();
    let word = seeded_word(&h).await;

    let (status, body) = get(&h.router, &format!("/api/words/{word}")).await;
    assert_eq!(status, StatusCode::OK);

    // `word` is nested, not flattened.
    assert_eq!(body["word"]["lemma"], "benevolent");
    assert_eq!(body["word"]["role"], "target");
    assert!(body["word"]["ready"].is_boolean());
    assert!(body["word"]["blockers"].is_array());

    // Definitions: one slot per pos, each with selection + candidates.
    let slot = &body["definitions"][0];
    assert_eq!(slot["pos"], "adj");
    assert_eq!(
        slot["selection"]["def_cand_id"],
        slot["candidates"][0]["def_cand_id"]
    );
    assert!(slot["selection"]["is_primary"].is_boolean());
    assert!(slot["selection"]["pinned"].is_boolean());
    assert!(slot["selection"]["approved"].is_boolean());
    assert!(slot["candidates"][0]["text_hash"].is_string());

    // Examples: always three slot views.
    let examples = body["examples"].as_array().unwrap();
    assert_eq!(examples.len(), 3);
    assert_eq!(examples[0]["slot"], 1);
    assert_eq!(examples[2]["slot"], 3);
    assert!(examples[0]["selection"].is_null());

    // Image: one slot view with a nullable selection.
    assert!(body["image"]["selection"].is_null());
    assert!(body["image"]["candidates"].is_array());

    // TTS: one entry per desired text, each content-addressed.
    let tts = body["tts"].as_array().unwrap();
    assert_eq!(tts.len(), 2);
    for entry in tts {
        assert_eq!(entry["status"], "missing");
        assert!(entry["input_hash"].as_str().unwrap().len() == 64);
        assert!(entry["text_hash"].as_str().unwrap().len() == 64);
        assert_eq!(entry["voice"], "en-US-AriaNeural");
        assert_eq!(entry["engine"], "edge-tts");
    }
    let word_entry = tts.iter().find(|e| e["kind"] == "word").unwrap();
    assert!(word_entry["ref"].is_null(), "the lemma belongs to no slot");
    let sense_entry = tts.iter().find(|e| e["kind"] == "definition").unwrap();
    assert_eq!(sense_entry["ref"]["pos"], "adj");

    assert!(body["distractors"].is_array());
    assert!(body["recent_events"].is_array());
}

/// Seed a `synth_tts` job_state row for one desired text.
async fn seed_tts_job(
    h: &Harness,
    kind: morpho_domain::types::TtsKind,
    text: &str,
    status: morpho_domain::job::JobStatus,
) -> String {
    use morpho_domain::job::{JobKey, JobKind, RateKey, SubjectRef};
    use morpho_store::ops::UpsertJobState;

    let input_hash = TtsConfig::default().input_hash(kind, text);
    h.store
        .write(
            Actor::Worker(JobKind::SynthTts),
            WriteOp::UpsertJobState(UpsertJobState {
                key: JobKey::new(JobKind::SynthTts, SubjectRef::tts_input(input_hash.clone())),
                rate_key: RateKey::EdgeTts,
                status,
                attempts: 5,
                next_retry_at: None,
                last_error: Some("edge-tts rejected the voice".into()),
            }),
        )
        .await
        .unwrap();
    input_hash
}

/// Ruling #13: a dead `synth_tts` row is `failed`, with or without an asset row.
#[tokio::test]
async fn a_dead_tts_job_makes_that_text_failed() {
    use morpho_domain::job::JobStatus;
    use morpho_domain::types::TtsKind;

    let h = harness();
    let word = seeded_word(&h).await;
    let hash = seed_tts_job(&h, TtsKind::Word, "benevolent", JobStatus::Dead).await;

    let (_, body) = get(&h.router, &format!("/api/words/{word}")).await;
    let tts = body["tts"].as_array().unwrap();
    let lemma = tts.iter().find(|e| e["input_hash"] == hash).unwrap();
    assert_eq!(lemma["status"], "failed", "no tts_assets row exists");
    assert_eq!(lemma["last_error"], "edge-tts rejected the voice");
    // The sense clip was never attempted, so it is still honestly `missing`.
    let sense = tts.iter().find(|e| e["kind"] == "definition").unwrap();
    assert_eq!(sense["status"], "missing");

    // And the dashboard rollup buckets it the same way.
    let (_, dashboard) = get(&h.router, "/api/dashboard").await;
    assert_eq!(dashboard["assets"]["tts"]["failed"], 1);
    assert_eq!(dashboard["assets"]["tts"]["missing"], 1);
}

/// Ruling #13: `missing` strictly means "not attempted, or still retrying".
#[tokio::test]
async fn a_tts_job_still_in_backoff_stays_missing() {
    use morpho_domain::job::JobStatus;
    use morpho_domain::types::TtsKind;

    let h = harness();
    let word = seeded_word(&h).await;
    let hash = seed_tts_job(&h, TtsKind::Word, "benevolent", JobStatus::Backoff).await;

    let (_, body) = get(&h.router, &format!("/api/words/{word}")).await;
    let entry = body["tts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["input_hash"] == hash)
        .unwrap()
        .clone();
    assert_eq!(entry["status"], "missing", "retries have not run out");

    let (_, dashboard) = get(&h.router, "/api/dashboard").await;
    assert_eq!(dashboard["assets"]["tts"]["failed"], 0);
}

/// A waived clip is the operator saying "no audio, on purpose" — also `failed`,
/// because that is what the word's blocker says.
#[tokio::test]
async fn a_waived_tts_job_is_failed_too() {
    use morpho_domain::job::JobStatus;
    use morpho_domain::types::TtsKind;

    let h = harness();
    let word = seeded_word(&h).await;
    let hash = seed_tts_job(&h, TtsKind::Word, "benevolent", JobStatus::Waived).await;

    let (_, body) = get(&h.router, &format!("/api/words/{word}")).await;
    let entry = body["tts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["input_hash"] == hash)
        .unwrap()
        .clone();
    assert_eq!(entry["status"], "failed");
}

#[tokio::test]
async fn an_unknown_word_is_a_404_envelope() {
    let h = harness();
    let (status, body) = get(&h.router, "/api/words/999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    assert!(body["error"]["message"].as_str().unwrap().contains("999"));
}

#[tokio::test]
async fn creating_a_word_returns_its_detail() {
    let h = harness();
    let (status, body) = post(
        &h.router,
        "/api/words",
        serde_json::json!({"lemma": "serene", "role": "target"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["word"]["lemma"], "serene");
    assert_eq!(body["word"]["created_by"], "manual");

    let (conflict, _) = post(
        &h.router,
        "/api/words",
        serde_json::json!({"lemma": "serene", "role": "target"}),
    )
    .await;
    assert_eq!(conflict, StatusCode::CONFLICT);
}

// ---------------------------------------------------------------------------
// Candidates and selections
// ---------------------------------------------------------------------------

#[tokio::test]
async fn minting_a_definition_returns_the_whole_word() {
    let h = harness();
    let word = seed_word(&h.store, "serene", Role::Target, Some(4602)).await;

    let (status, body) = post(
        &h.router,
        "/api/candidates/definition",
        serde_json::json!({"word_id": word, "pos": "adj", "text": "calm and peaceful"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    // Ruling #2: the full WordDetail, not the row.
    assert_eq!(body["word"]["word_id"], word);
    let candidate = &body["definitions"][0]["candidates"][0];
    assert_eq!(candidate["text"], "calm and peaceful");
    assert_eq!(candidate["source"], "manual");
}

/// The highlight is computed from the word and the stored text, not taken from
/// the caller.
///
/// A console measures offsets against the string in its textarea; the row holds
/// the *canonicalized* string, and a leading space or a double space between
/// words shifts every offset after it. Trusting the caller is how a highlight
/// ends up one character off, or two, on exactly the sentences somebody typed
/// by hand.
#[tokio::test]
async fn minting_an_example_recomputes_the_highlight() {
    let h = harness();
    let word = seed_word(&h.store, "serene", Role::Target, Some(4602)).await;

    // Offsets measured against the raw caption: "serene" starts at byte 4
    // there, and at byte 2 of the canonical text the row will hold.
    let (status, body) = post(
        &h.router,
        "/api/candidates/example",
        serde_json::json!({
            "word_id": word,
            "text": "  A  serene   lake lay below.",
            "hl_start": 5,
            "hl_end": 11
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let candidate = &body["examples"][0]["candidates"][0];
    let text = candidate["text"].as_str().unwrap();
    assert_eq!(text, "A serene lake lay below.");
    let (start, end) = (
        candidate["hl_start"].as_u64().unwrap() as usize,
        candidate["hl_end"].as_u64().unwrap() as usize,
    );
    assert_eq!(
        &text[start..end],
        "serene",
        "the stored offsets have to index the stored text"
    );

    // A sentence that does not contain the word has no highlight to compute,
    // and one is never guessed.
    let (bad, body) = post(
        &h.router,
        "/api/candidates/example",
        serde_json::json!({
            "word_id": word,
            "text": "Short and quite unrelated.",
            "hl_start": 0,
            "hl_end": 5
        }),
    )
    .await;
    assert_eq!(bad, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "unprocessable");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("serene"));
}

#[tokio::test]
async fn selection_override_pins_and_returns_detail() {
    let h = harness();
    let word = seed_word(&h.store, "serene", Role::Target, Some(4602)).await;
    let first = seed_definition(&h.store, word, "adj", "calm and peaceful").await;
    let second = seed_definition(&h.store, word, "adj", "free from disturbance").await;
    select_definition(&h.store, word, "adj", first).await;

    let (status, body) = post(
        &h.router,
        "/api/selections/definition",
        serde_json::json!({"word_id": word, "pos": "adj", "cand_id": second}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let selection = &body["definitions"][0]["selection"];
    assert_eq!(selection["def_cand_id"], second);
    assert_eq!(selection["selected_by"], "human");
    assert_eq!(selection["pinned"], true);
    assert_eq!(selection["selection_rev"], 2);
}

/// An automated caller (e.g. ops/verify_genimg.py) can select a candidate
/// without pinning it, so a later, better-scoring candidate can still take the
/// slot back — the whole point of not locking an automated pick in place.
#[tokio::test]
async fn selection_override_can_opt_out_of_pinning() {
    let h = harness();
    let word = seed_word(&h.store, "serene", Role::Target, Some(4602)).await;
    let first = seed_definition(&h.store, word, "adj", "calm and peaceful").await;
    let second = seed_definition(&h.store, word, "adj", "free from disturbance").await;
    select_definition(&h.store, word, "adj", first).await;

    let (status, body) = post(
        &h.router,
        "/api/selections/definition",
        serde_json::json!({"word_id": word, "pos": "adj", "cand_id": second, "pin": false}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let selection = &body["definitions"][0]["selection"];
    assert_eq!(selection["def_cand_id"], second);
    assert_eq!(selection["pinned"], false);
}

#[tokio::test]
async fn approval_round_trips_and_records_the_actor() {
    let h = harness();
    let word = seeded_word(&h).await;

    let (status, body) = post(
        &h.router,
        "/api/selections/definition/approve",
        serde_json::json!({"word_id": word, "pos": "adj"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let selection = &body["definitions"][0]["selection"];
    assert_eq!(selection["approved"], true);
    assert_eq!(selection["approved_by"], "admin:abyss");
    assert_eq!(selection["pinned"], true, "approval implies a pin");
    assert!(selection["approved_hash"].is_string());

    let (status, body) = delete(
        &h.router,
        "/api/selections/definition/approve",
        serde_json::json!({"word_id": word, "pos": "adj"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["definitions"][0]["selection"]["approved"], false);
    assert!(body["definitions"][0]["selection"]["approved_hash"].is_null());
}

#[tokio::test]
async fn primary_and_enabled_move_independently() {
    let h = harness();
    let word = seed_word(&h.store, "record", Role::Target, Some(900)).await;
    let noun = seed_definition(&h.store, word, "noun", "a written account").await;
    let verb = seed_definition(&h.store, word, "verb", "to write an account").await;
    select_definition(&h.store, word, "noun", noun).await;
    select_definition(&h.store, word, "verb", verb).await;

    let (status, body) = post(
        &h.router,
        "/api/selections/definition/primary",
        serde_json::json!({"word_id": word, "pos": "verb"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let slots = body["definitions"].as_array().unwrap();
    // Primary sense first (contract: "grouped by pos, primary sense first").
    assert_eq!(slots[0]["pos"], "verb");
    assert_eq!(slots[0]["selection"]["is_primary"], true);
    assert_eq!(slots[1]["selection"]["is_primary"], false);

    let (status, body) = post(
        &h.router,
        "/api/selections/definition/enabled",
        serde_json::json!({"word_id": word, "pos": "noun", "enabled": false}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let noun_slot = body["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|slot| slot["pos"] == "noun")
        .unwrap();
    assert_eq!(noun_slot["selection"]["enabled"], false);
}

#[tokio::test]
async fn rejecting_the_selected_candidate_clears_pin_and_approval() {
    let h = harness();
    let word = seeded_word(&h).await;
    post(
        &h.router,
        "/api/selections/definition/approve",
        serde_json::json!({"word_id": word, "pos": "adj"}),
    )
    .await;
    let cand = {
        let (_, body) = get(&h.router, &format!("/api/words/{word}")).await;
        body["definitions"][0]["selection"]["def_cand_id"]
            .as_i64()
            .unwrap()
    };

    let (status, body) = post(
        &h.router,
        &format!("/api/candidates/definition/{cand}/reject"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let slot = &body["definitions"][0];
    assert_eq!(slot["candidates"][0]["status"], "rejected");
    assert_eq!(slot["selection"]["pinned"], false);
    assert_eq!(slot["selection"]["approved"], false);
}

#[tokio::test]
async fn an_unknown_kind_is_rejected() {
    let h = harness();
    let (status, body) = post(
        &h.router,
        "/api/selections/nonsense",
        serde_json::json!({"word_id": 1, "cand_id": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

// ---------------------------------------------------------------------------
// OOV queue
// ---------------------------------------------------------------------------

/// Seed a word whose selected definition contains an out-of-scope lemma, with
/// the tokens extracted so the views fire.
async fn seed_oov(h: &Harness) -> (i64, i64) {
    let word = seed_word(&h.store, "benevolent", Role::Target, Some(4312)).await;
    seed_word(&h.store, "kind", Role::Base, None).await;
    seed_word(&h.store, "and", Role::Base, None).await;
    let cand = seed_definition(&h.store, word, "adj", "kind and altruistic").await;
    select_definition(&h.store, word, "adj", cand).await;

    let pipeline = morpho_reconcile::TextPipeline::default();
    let tokens = pipeline.extract("kind and altruistic");
    h.store
        .write(
            Actor::Reconciler,
            WriteOp::record_extraction(
                cand,
                morpho_domain::hash::text_hash("kind and altruistic"),
                pipeline.input_hash(&morpho_domain::hash::text_hash("kind and altruistic")),
                pipeline.tokenizer_ver().to_string(),
                pipeline.lemmatizer_ver().to_string(),
                tokens,
            ),
        )
        .await
        .unwrap();
    h.store
        .write(
            Actor::Reconciler,
            WriteOp::SyncOosQueue(morpho_store::ops::SyncOosQueue {
                present: vec!["altruistic".to_string()],
            }),
        )
        .await
        .unwrap();
    (word, cand)
}

#[tokio::test]
async fn oov_queue_carries_occurrences() {
    let h = harness();
    let (word, cand) = seed_oov(&h).await;

    let (status, body) = get(&h.router, "/api/oov?status=open").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 1);
    let entry = &body["items"][0];
    assert_eq!(entry["oos_lemma"], "altruistic");
    assert_eq!(entry["status"], "open");
    assert_eq!(entry["occurrence_count"], 1);
    let occurrence = &entry["occurrences"][0];
    assert_eq!(occurrence["word_id"], word);
    assert_eq!(occurrence["lemma"], "benevolent");
    assert_eq!(occurrence["pos"], "adj");
    assert_eq!(occurrence["def_cand_id"], cand);
    assert_eq!(occurrence["text"], "kind and altruistic");
    assert_eq!(occurrence["hits"], 1);
    // No rewrite drafted yet.
    assert!(occurrence["suggested_rewrite"].is_null());
}

#[tokio::test]
async fn a_drafted_rewrite_surfaces_as_a_suggestion() {
    let h = harness();
    let (word, cand) = seed_oov(&h).await;
    h.store
        .write(
            Actor::Worker(morpho_domain::JobKind::RewriteDefinition),
            WriteOp::MintDefinitionCandidate(morpho_store::ops::MintDefinitionCandidate {
                word_id: word,
                pos: "adj".into(),
                text: "kind and generous".into(),
                source: DefinitionSource::LlmRewrite,
                source_ref: None,
                parent_cand_id: Some(cand),
                created_by: None,
                select: false,
            }),
        )
        .await
        .unwrap();

    let (_, body) = get(&h.router, "/api/oov?status=open").await;
    assert_eq!(
        body["items"][0]["occurrences"][0]["suggested_rewrite"],
        "kind and generous"
    );
}

#[tokio::test]
async fn resolving_by_promotion_creates_an_auxiliary() {
    let h = harness();
    seed_oov(&h).await;

    let (status, body) = post(
        &h.router,
        "/api/oov/altruistic/resolve",
        serde_json::json!({"mode": "promote", "notes": "needed by benevolent"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // The queue answer no longer lists it as open.
    assert_eq!(body["total"], 0);

    let (_, words) = get(&h.router, "/api/words?role=auxiliary").await;
    assert_eq!(words["total"], 1);
    assert_eq!(words["items"][0]["lemma"], "altruistic");
}

#[tokio::test]
async fn resolving_by_rewrite_selects_the_new_candidate() {
    let h = harness();
    let (word, cand) = seed_oov(&h).await;

    let (status, _) = post(
        &h.router,
        "/api/oov/altruistic/resolve",
        serde_json::json!({"mode": "rewrite", "def_cand_id": cand, "text": "kind and giving"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, detail) = get(&h.router, &format!("/api/words/{word}")).await;
    let selection = &detail["definitions"][0]["selection"];
    assert_ne!(selection["def_cand_id"], cand);
    assert_eq!(selection["selected_by"], "human");
}

#[tokio::test]
async fn resolving_by_gloss_anchors_the_lemma() {
    let h = harness();
    let (word, _) = seed_oov(&h).await;

    let (status, body) = post(
        &h.router,
        "/api/oov/altruistic/resolve",
        serde_json::json!({"mode": "gloss", "zh_gloss": "利他的", "notes": "not worth teaching"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 0, "the entry left the open queue");

    // The anchor exists as an active auxiliary carrying the gloss.
    let (_, words) = get(&h.router, "/api/words?q=altruistic").await;
    assert_eq!(words["total"], 1);
    let anchor = words["items"][0]["word_id"].as_i64().unwrap();
    let (_, detail) = get(&h.router, &format!("/api/words/{anchor}")).await;
    assert_eq!(detail["word"]["role"], "auxiliary");
    assert_eq!(detail["word"]["aux_status"], "active");
    assert_eq!(detail["word"]["zh_gloss"], "利他的");
    assert_eq!(detail["word"]["zh_gloss_source"], "manual");
    // An anchor is never spoken, so it lists no clips to chase.
    assert_eq!(detail["tts"].as_array().unwrap().len(), 0);

    // The queue row records how it was closed.
    let (_, resolved) = get(&h.router, "/api/oov?status=resolved_gloss").await;
    assert_eq!(resolved["items"][0]["oos_lemma"], "altruistic");
    assert_eq!(resolved["items"][0]["status"], "resolved_gloss");

    // And the word that needed it is untouched.
    let (status, _) = get(&h.router, &format!("/api/words/{word}")).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn resolving_by_gloss_rejects_an_empty_gloss() {
    let h = harness();
    seed_oov(&h).await;
    let (status, body) = post(
        &h.router,
        "/api/oov/altruistic/resolve",
        serde_json::json!({"mode": "gloss", "zh_gloss": "   "}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

// ---------------------------------------------------------------------------
// Gloss anchors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn setting_and_clearing_a_gloss_returns_the_whole_word() {
    let h = harness();
    let word = seed_word(&h.store, "perambulate", Role::Target, Some(41_000)).await;

    let (status, body) = post(
        &h.router,
        &format!("/api/words/{word}/gloss"),
        serde_json::json!({"zh_gloss": "漫步"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["word"]["word_id"], word);
    assert_eq!(body["word"]["zh_gloss"], "漫步");
    assert_eq!(body["word"]["zh_gloss_source"], "manual");
    // Wave-2 ruling #2: the whole detail comes back, not just the row.
    assert!(body["definitions"].is_array());
    assert!(body["distractors"].is_array());

    let (status, body) = delete(
        &h.router,
        &format!("/api/words/{word}/gloss"),
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["word"]["zh_gloss"].is_null());
    assert!(body["word"]["zh_gloss_source"].is_null());
}

#[tokio::test]
async fn a_gloss_is_audited_both_ways() {
    let h = harness();
    let word = seed_word(&h.store, "perambulate", Role::Target, Some(41_000)).await;
    post(
        &h.router,
        &format!("/api/words/{word}/gloss"),
        serde_json::json!({"zh_gloss": "漫步"}),
    )
    .await;
    delete(
        &h.router,
        &format!("/api/words/{word}/gloss"),
        serde_json::Value::Null,
    )
    .await;

    let (_, events) = get(
        &h.router,
        &format!("/api/events?entity_type=word&entity_id={word}"),
    )
    .await;
    let actions: Vec<&str> = events["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["action"].as_str().unwrap())
        .collect();
    assert!(actions.contains(&"gloss_set"), "{actions:?}");
    assert!(actions.contains(&"gloss_cleared"), "{actions:?}");
    let set = events["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["action"] == "gloss_set")
        .unwrap();
    assert_eq!(set["actor"], "admin:abyss");
    assert_eq!(set["detail"]["zh_gloss"], "漫步");
}

#[tokio::test]
async fn an_empty_gloss_is_refused_and_an_unknown_word_is_not_found() {
    let h = harness();
    let word = seed_word(&h.store, "perambulate", Role::Target, Some(41_000)).await;

    let (status, _) = post(
        &h.router,
        &format!("/api/words/{word}/gloss"),
        serde_json::json!({"zh_gloss": ""}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = post(
        &h.router,
        "/api/words/9999/gloss",
        serde_json::json!({"zh_gloss": "漫步"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Gallery
// ---------------------------------------------------------------------------

/// A target word with one selected image candidate, optionally scored against
/// the word's own query text (its lemma, since none of these words get a
/// slot-1 example — matching the fallback branch of
/// `morpho_reconcile::facts::clip_queries`).
async fn seed_gallery_word(h: &Harness, lemma: &str, rank: i64, similarity: Option<f64>) -> i64 {
    let word = seed_word(&h.store, lemma, Role::Target, Some(rank)).await;
    let file_hash = format!("hash-{lemma}");

    let cand_id = h
        .store
        .write(
            Actor::Cli,
            WriteOp::MintImageCandidate(MintImageCandidate {
                word_id: word,
                pos: None,
                file_hash: file_hash.clone(),
                media: Some(MediaRegistration {
                    file_hash: file_hash.clone(),
                    kind: MediaKind::Image,
                    rel_path: format!("media/{file_hash}.webp"),
                    bytes: 1024,
                }),
                width: Some(512),
                height: Some(512),
                source: ImageSource::Manual,
                source_ref: None,
                license: None,
                query_used: None,
                created_by: None,
            }),
        )
        .await
        .unwrap()
        .result
        .cand_id()
        .unwrap();

    h.store
        .write(
            Actor::Reconciler,
            WriteOp::select(SlotRef::Image { word_id: word }, cand_id, SelectedBy::Auto),
        )
        .await
        .unwrap();

    if let Some(similarity) = similarity {
        let mut images_cfg = morpho_reconcile::ImagesConfig::default();
        images_cfg.apply_env();
        h.store
            .write(
                Actor::Cli,
                WriteOp::ApplyClipScores(ApplyClipScores {
                    model_ver: images_cfg.clip_model_ver(),
                    rows: vec![ClipScoreRow {
                        file_hash,
                        text_hash: morpho_domain::text_hash(lemma),
                        similarity,
                    }],
                    touched_words: vec![word],
                }),
            )
            .await
            .unwrap();
    }

    word
}

#[tokio::test]
async fn gallery_exposes_clip_similarity_and_null_when_unscored() {
    let h = harness();
    seed_gallery_word(&h, "lucent", 100, Some(0.314)).await;
    seed_gallery_word(&h, "murky", 200, None).await;

    let (status, body) = get(&h.router, "/api/gallery").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 2);

    let items = body["items"].as_array().unwrap();
    let lucent = items.iter().find(|item| item["lemma"] == "lucent").unwrap();
    assert!((lucent["clip_similarity"].as_f64().unwrap() - 0.314).abs() < 1e-9);

    let murky = items.iter().find(|item| item["lemma"] == "murky").unwrap();
    assert!(murky["clip_similarity"].is_null());
}

#[tokio::test]
async fn gallery_exposes_the_slot1_sentence_and_null_when_unselected() {
    let h = harness();
    let with_sentence = seed_gallery_word(&h, "lucent", 100, None).await;
    let cand_id = h
        .store
        .write(
            Actor::Cli,
            WriteOp::MintExampleCandidate(MintExampleCandidate {
                word_id: with_sentence,
                text: "The lucent moon lit the path.".into(),
                hl_start: 4,
                hl_end: 10,
                source: ExampleSource::Manual,
                source_ref: None,
                created_by: None,
            }),
        )
        .await
        .unwrap()
        .result
        .cand_id()
        .unwrap();
    h.store
        .write(
            Actor::Reconciler,
            WriteOp::select(
                SlotRef::Example {
                    word_id: with_sentence,
                    slot: 1,
                },
                cand_id,
                SelectedBy::Auto,
            ),
        )
        .await
        .unwrap();

    seed_gallery_word(&h, "murky", 200, None).await;

    let (status, body) = get(&h.router, "/api/gallery").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let items = body["items"].as_array().unwrap();
    let lucent = items.iter().find(|item| item["lemma"] == "lucent").unwrap();
    assert_eq!(lucent["slot1_sentence"], "The lucent moon lit the path.");

    let murky = items.iter().find(|item| item["lemma"] == "murky").unwrap();
    assert!(murky["slot1_sentence"].is_null());
}

#[tokio::test]
async fn gallery_sort_clip_asc_puts_the_worst_match_first_with_nulls_last() {
    let h = harness();
    seed_gallery_word(&h, "radiant", 100, Some(0.81)).await;
    seed_gallery_word(&h, "dim", 200, Some(0.12)).await;
    seed_gallery_word(&h, "vague", 300, None).await;

    let (status, body) = get(&h.router, "/api/gallery?sort=clip_asc").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 3);

    let lemmas: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["lemma"].as_str().unwrap())
        .collect();
    // Worst semantic match first, best last, the unscored pair after both —
    // it is not "worse" than 0.12, it simply has no answer yet.
    assert_eq!(lemmas, vec!["dim", "radiant", "vague"]);
}

#[tokio::test]
async fn gallery_sort_clip_desc_reverses_scored_order_and_paginates_by_offset() {
    let h = harness();
    seed_gallery_word(&h, "radiant", 100, Some(0.81)).await;
    seed_gallery_word(&h, "dim", 200, Some(0.12)).await;
    seed_gallery_word(&h, "vague", 300, None).await;

    let (status, body) = get(&h.router, "/api/gallery?sort=clip_desc&page=1&page_size=2").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 3);
    let page1: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["lemma"].as_str().unwrap())
        .collect();
    assert_eq!(page1, vec!["radiant", "dim"]);

    let (status, body) = get(&h.router, "/api/gallery?sort=clip_desc&page=2&page_size=2").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 3);
    let page2: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["lemma"].as_str().unwrap())
        .collect();
    assert_eq!(page2, vec!["vague"]);
}

// ---------------------------------------------------------------------------
// Dead letters
// ---------------------------------------------------------------------------

async fn seed_dead_letter(h: &Harness, word: i64) {
    use morpho_domain::job::{JobKey, JobKind, JobStatus, RateKey, SubjectRef};
    use morpho_store::ops::UpsertJobState;

    h.store
        .write(
            Actor::Worker(JobKind::FetchImages),
            WriteOp::UpsertJobState(UpsertJobState {
                key: JobKey::new(
                    JobKind::FetchImages,
                    SubjectRef::word_source(word, "unsplash"),
                ),
                rate_key: RateKey::Unsplash,
                status: JobStatus::Dead,
                attempts: 8,
                next_retry_at: None,
                last_error: Some("unsplash has no API key configured".into()),
            }),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn dead_letters_join_their_subject() {
    let h = harness();
    let word = seed_word(&h.store, "serene", Role::Target, Some(4602)).await;
    seed_dead_letter(&h, word).await;

    let (status, body) = get(&h.router, "/api/dead-letters").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 1);
    let letter = &body["items"][0];
    assert_eq!(letter["kind"], "fetch_images");
    assert_eq!(letter["subject_type"], "word");
    assert_eq!(letter["subject_id"], format!("{word}:unsplash"));
    assert_eq!(letter["status"], "dead");
    assert_eq!(letter["attempts"], 8);
    assert_eq!(letter["subject"]["word_id"], word);
    assert_eq!(letter["subject"]["lemma"], "serene");
    assert_eq!(letter["subject"]["label"], "serene (unsplash)");
}

#[tokio::test]
async fn dead_letters_paginate() {
    let h = harness();
    for lemma in ["serene", "tranquil", "lucid"] {
        let word = seed_word(&h.store, lemma, Role::Target, Some(1000)).await;
        seed_dead_letter(&h, word).await;
    }
    let (_, body) = get(&h.router, "/api/dead-letters?page=1&page_size=2").await;
    assert_eq!(
        body["total"], 3,
        "the envelope always reports the full total"
    );
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
}

/// Ruling #14: an optional lane filter, with unknown values handled gracefully.
#[tokio::test]
async fn dead_letters_filter_by_rate_key() {
    use morpho_domain::job::{JobKey, JobKind, JobStatus, RateKey, SubjectRef};
    use morpho_store::ops::UpsertJobState;

    let h = harness();
    let word = seed_word(&h.store, "serene", Role::Target, Some(4602)).await;
    seed_dead_letter(&h, word).await; // unsplash lane
    h.store
        .write(
            Actor::Worker(JobKind::SynthTts),
            WriteOp::UpsertJobState(UpsertJobState {
                key: JobKey::new(JobKind::SynthTts, SubjectRef::tts_input("abc")),
                rate_key: RateKey::EdgeTts,
                status: JobStatus::Dead,
                attempts: 5,
                next_retry_at: None,
                last_error: Some("voice not available".into()),
            }),
        )
        .await
        .unwrap();

    let (status, all) = get(&h.router, "/api/dead-letters").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["total"], 2);

    let (status, lane) = get(&h.router, "/api/dead-letters?rate_key=edge_tts").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(lane["total"], 1);
    assert_eq!(lane["items"][0]["rate_key"], "edge_tts");
    assert_eq!(lane["items"][0]["kind"], "synth_tts");

    // A real lane with nothing dead in it.
    let (status, empty) = get(&h.router, "/api/dead-letters?rate_key=pexels").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["total"], 0);

    // An unknown value matches nothing rather than erroring or widening.
    let (status, unknown) = get(&h.router, "/api/dead-letters?rate_key=not-a-lane").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unknown["total"], 0);
    assert!(unknown["items"].as_array().unwrap().is_empty());

    // A blank value is no filter at all.
    let (status, blank) = get(&h.router, "/api/dead-letters?rate_key=").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(blank["total"], 2);
}

#[tokio::test]
async fn the_rate_key_filter_composes_with_pagination() {
    let h = harness();
    for lemma in ["serene", "tranquil", "lucid"] {
        let word = seed_word(&h.store, lemma, Role::Target, Some(1000)).await;
        seed_dead_letter(&h, word).await;
    }
    let (_, body) = get(
        &h.router,
        "/api/dead-letters?rate_key=unsplash&page=2&page_size=2",
    )
    .await;
    assert_eq!(body["total"], 3);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn retry_deletes_the_row_and_waive_flips_it() {
    let h = harness();
    let word = seed_word(&h.store, "serene", Role::Target, Some(4602)).await;
    seed_dead_letter(&h, word).await;
    let key = serde_json::json!({
        "kind": "fetch_images",
        "subject_type": "word",
        "subject_id": format!("{word}:unsplash")
    });

    let (status, body) = post(&h.router, "/api/dead-letters/retry", key.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 0, "retry means the need is derivable again");

    seed_dead_letter(&h, word).await;
    let (status, body) = post(&h.router, "/api/dead-letters/waive", key).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 0, "a waived row is no longer a dead letter");

    let status: String = h
        .store
        .read(|conn| {
            Ok(conn.query_row("SELECT status FROM job_state", [], |row| {
                row.get::<_, String>(0)
            })?)
        })
        .await
        .unwrap();
    assert_eq!(status, "waived");
}

#[tokio::test]
async fn waiving_an_unknown_job_is_a_404() {
    let h = harness();
    let (status, _) = post(
        &h.router,
        "/api/dead-letters/waive",
        serde_json::json!({"kind": "fetch_images", "subject_type": "word", "subject_id": "42"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

async fn seed_plan(h: &Harness) -> Vec<i64> {
    use morpho_store::ops::{PlanGroupRow, PlanWordRow, WritePlan};

    let mut ids = Vec::new();
    for (index, lemma) in ["kind", "generous", "benevolent"].iter().enumerate() {
        ids.push(seed_word(&h.store, lemma, Role::Target, Some(index as i64 * 100)).await);
    }
    h.store
        .write(
            Actor::Reconciler,
            WriteOp::WritePlan(WritePlan {
                input_hash: "hash-1".into(),
                algo_ver: "plan/1".into(),
                params_json: r#"{"group_min":15,"group_max":20}"#.into(),
                stats_json: r#"{"word_count":3,"group_count":1,"edge_count":0,
                                "scc_group_count":0,"largest_group":3,"avg_group_size":3.0}"#
                    .into(),
                groups: vec![PlanGroupRow {
                    group_seq: 1,
                    group_type: "fill".into(),
                }],
                words: ids
                    .iter()
                    .enumerate()
                    .map(|(index, word_id)| PlanWordRow {
                        word_id: *word_id,
                        learning_order: index as i64 + 1,
                        group_seq: 1,
                    })
                    .collect(),
            }),
        )
        .await
        .unwrap();
    ids
}

#[tokio::test]
async fn plan_summary_matches_the_wire_shape() {
    let h = harness();
    seed_plan(&h).await;

    let (status, body) = get(&h.router, "/api/plan").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["input_hash"], "hash-1");
    assert_eq!(body["algo_ver"], "plan/1");
    assert_eq!(body["is_current"], true);
    assert_eq!(body["params"]["group_max"], 20);
    assert_eq!(body["stats"]["word_count"], 3);
    assert_eq!(body["stats"]["largest_group"], 3);

    let group = &body["groups"][0];
    assert_eq!(group["group_seq"], 1);
    assert_eq!(group["group_type"], "fill");
    assert_eq!(group["word_count"], 3);
    assert_eq!(group["ready_count"], 0);
    assert_eq!(group["first_lemma"], "kind");
    assert_eq!(group["last_lemma"], "benevolent");

    // First plan: everything is an addition.
    assert!(body["diff"]["previous_plan_id"].is_null());
    assert_eq!(body["diff"]["added"], 3);
}

#[tokio::test]
async fn plan_group_detail_lists_words_in_order() {
    let h = harness();
    let ids = seed_plan(&h).await;

    let (status, body) = get(&h.router, "/api/plan/groups/1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["group_seq"], 1);
    assert_eq!(body["group_type"], "fill");
    let words = body["words"].as_array().unwrap();
    assert_eq!(words.len(), 3);
    assert_eq!(words[0]["word_id"], ids[0]);
    assert_eq!(words[0]["learning_order"], 1);
    assert!(words[0]["ready"].is_boolean());
    assert!(words[0]["blockers"].is_array());
}

#[tokio::test]
async fn plan_endpoints_404_before_the_first_build() {
    let h = harness();
    assert_eq!(get(&h.router, "/api/plan").await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        get(&h.router, "/api/plan/groups/1").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn an_unknown_group_is_a_404() {
    let h = harness();
    seed_plan(&h).await;
    assert_eq!(
        get(&h.router, "/api/plan/groups/99").await.0,
        StatusCode::NOT_FOUND
    );
}

// ---------------------------------------------------------------------------
// Releases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn release_history_starts_empty() {
    let h = harness();
    let (status, body) = get(&h.router, "/api/releases").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 0);
    assert!(body["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_holdback_report_explains_every_word() {
    let h = harness();
    let ids = seed_plan(&h).await;

    let (status, body) = get(&h.router, "/api/releases/preview").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["shippable_count"], 0);
    assert_eq!(body["exportable_count"], 0);
    assert_eq!(body["excluded_count"], ids.len());
    assert_eq!(body["gates_pass"], true, "an empty cut breaks no invariant");
    assert!(body["gate_failures"].as_array().unwrap().is_empty());

    let excluded = body["excluded"].as_array().unwrap();
    assert_eq!(excluded.len(), ids.len());
    for entry in excluded {
        assert!(entry["lemma"].is_string());
        assert!(!entry["root_cause"].as_str().unwrap().is_empty());
        assert!(!entry["root_cause_detail"].as_str().unwrap().is_empty());
        assert!(entry["impact_count"].is_number());
    }
}

#[tokio::test]
async fn preview_before_a_plan_is_a_404() {
    let h = harness();
    let (status, _) = get(&h.router, "/api/releases/preview").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn exporting_an_empty_cut_succeeds_and_records_a_release() {
    let h = harness();
    seed_plan(&h).await;

    let (status, body) = post(&h.router, "/api/releases/export", serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["release"]["word_count"], 0);
    assert_eq!(body["release"]["media_count"], 0);
    assert!(body["release"]["version"].as_str().unwrap().contains('+'));
    assert_eq!(body["holdback"]["excluded_count"], 3);

    let (_, history) = get(&h.router, "/api/releases").await;
    assert_eq!(history["total"], 1);
    assert_eq!(history["items"][0]["word_count"], 0);
    assert_eq!(history["items"][0]["exported_by"], "abyss");
}

/// Ruling #15: `Release.word_count` is a column of `releases`, so the history
/// survives an audit log that no longer carries the export event.
#[tokio::test]
async fn release_history_reads_the_word_count_column() {
    use morpho_store::ops::RecordRelease;

    let h = harness();
    seed_plan(&h).await;
    let plan_id: i64 = h
        .store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT plan_id FROM plan_artifacts WHERE is_current = 1",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    h.store
        .write(
            Actor::Cli,
            WriteOp::RecordRelease(RecordRelease {
                version: "2026.08.26+deadbeef".into(),
                plan_id,
                input_hash: "input-1".into(),
                db_file_hash: "db-1".into(),
                exported_by: "abyss".into(),
                notes: None,
                media_hashes: Vec::new(),
                word_count: 42,
            }),
        )
        .await
        .unwrap();

    let stored: i64 = h
        .store
        .read(|conn| Ok(conn.query_row("SELECT word_count FROM releases", [], |row| row.get(0))?))
        .await
        .unwrap();
    assert_eq!(stored, 42, "the exporter writes the column");

    // Erase the export event: the old implementation reconstructed the count
    // from it, so this is what makes the assertion below meaningful.
    let conn = rusqlite::Connection::open(h.data_dir.join("working.db")).unwrap();
    conn.execute("DELETE FROM events WHERE entity_type = 'release'", [])
        .unwrap();
    drop(conn);

    let (status, body) = get(&h.router, "/api/releases").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["word_count"], 42);
    assert_eq!(body["items"][0]["media_count"], 0);
}

#[tokio::test]
async fn exporting_before_a_plan_is_a_conflict() {
    let h = harness();
    let (status, body) = post(&h.router, "/api/releases/export", serde_json::json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
}

// ---------------------------------------------------------------------------
// Media
// ---------------------------------------------------------------------------

#[tokio::test]
async fn media_rejects_a_malformed_hash() {
    let h = harness();
    let (status, body) = get(&h.router, "/api/media/not-a-hash").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn media_404s_for_an_unregistered_hash() {
    let h = harness();
    let (status, _) = get(&h.router, &format!("/api/media/{}", "a".repeat(64))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn media_serves_registered_bytes_with_immutable_caching() {
    let h = harness();
    let media = morpho_store::MediaStore::new(h._dir.path());
    let stored = media
        .put_bytes(b"RIFF----WEBPfake", morpho_domain::types::MediaKind::Image)
        .unwrap();
    h.store
        .write(
            Actor::Cli,
            WriteOp::RegisterMediaFile {
                file_hash: stored.file_hash.clone(),
                kind: morpho_domain::types::MediaKind::Image,
                rel_path: stored.rel_path,
                bytes: stored.bytes,
            },
        )
        .await
        .unwrap();

    let response = h
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/media/{}", stored.file_hash))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/webp");
    assert!(response.headers()["cache-control"]
        .to_str()
        .unwrap()
        .contains("immutable"));
    let bytes = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"RIFF----WEBPfake");
}

// ---------------------------------------------------------------------------
// Cross-cutting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_actor_header_attributes_the_edit() {
    let h = harness();
    let word = seeded_word(&h).await;
    post(
        &h.router,
        "/api/selections/definition/approve",
        serde_json::json!({"word_id": word, "pos": "adj"}),
    )
    .await;

    let (_, body) = get(&h.router, "/api/events?action=approved").await;
    assert_eq!(body["items"][0]["actor"], "admin:abyss");
}

#[tokio::test]
async fn a_missing_actor_header_falls_back_to_local() {
    let h = harness();
    let word = seeded_word(&h).await;
    call(
        &h.router,
        Request::builder()
            .method("POST")
            .uri("/api/selections/definition/approve")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({"word_id": word, "pos": "adj"}).to_string(),
            ))
            .unwrap(),
    )
    .await;

    let (_, body) = get(&h.router, "/api/events?action=approved").await;
    assert_eq!(body["items"][0]["actor"], "admin:local");
}

#[tokio::test]
async fn every_contract_endpoint_is_implemented() {
    let h = harness();
    let word = seed_word(&h.store, "serene", Role::Target, Some(4602)).await;
    h.store
        .write(
            Actor::Cli,
            WriteOp::MintExampleCandidate(MintExampleCandidate {
                word_id: word,
                text: "A serene lake lay below.".into(),
                hl_start: 2,
                hl_end: 8,
                source: ExampleSource::Manual,
                source_ref: None,
                created_by: None,
            }),
        )
        .await
        .unwrap();

    // Nothing in the contract may answer 501 any more.
    for uri in [
        "/api/dashboard",
        "/api/events",
        "/api/jobs",
        "/api/words",
        "/api/oov",
        "/api/dead-letters",
        "/api/releases",
    ] {
        let (status, _) = get(&h.router, uri).await;
        assert_eq!(status, StatusCode::OK, "GET {uri}");
    }
    for (uri, body) in [
        (
            "/api/words",
            serde_json::json!({"lemma": "tranquil", "role": "target"}),
        ),
        (
            "/api/candidates/definition",
            serde_json::json!({"word_id": word, "pos": "adj", "text": "calm and peaceful"}),
        ),
        (
            "/api/candidates/example",
            // "It was serene." — bytes 7..13 cover the target word.
            serde_json::json!({"word_id": word, "text": "It was serene.", "hl_start": 7, "hl_end": 13}),
        ),
    ] {
        let (status, payload) = post(&h.router, uri, body).await;
        assert_eq!(status, StatusCode::CREATED, "POST {uri}: {payload}");
    }
    // Ruling #18a's pair, which answers with the word rather than creating one.
    let gloss = format!("/api/words/{word}/gloss");
    let (status, payload) =
        post(&h.router, &gloss, serde_json::json!({"zh_gloss": "宁静的"})).await;
    assert_eq!(status, StatusCode::OK, "POST {gloss}: {payload}");
    let (status, payload) = delete(&h.router, &gloss, serde_json::Value::Null).await;
    assert_eq!(status, StatusCode::OK, "DELETE {gloss}: {payload}");
}

// ---------------------------------------------------------------------------
// Distractor repair
// ---------------------------------------------------------------------------

const REBIND: &str = "/api/distractors/rebind-violations";

/// Mark words shippable, the way a reconciler pass would.
async fn set_core_ready(store: &Store, word_ids: &[i64]) {
    let rows = word_ids
        .iter()
        .map(|word_id| ReadinessRow {
            word_id: *word_id,
            ready: true,
            core_ready: true,
            blockers_json: "[]".to_string(),
        })
        .collect();
    store
        .write(
            Actor::Reconciler,
            WriteOp::ApplyReadiness(ApplyReadiness { rows }),
        )
        .await
        .unwrap();
}

/// `adapt` bound to its own derivation `adapter` — a binding the current rule
/// would never have made — with `adopt` and `adept` available to replace it.
async fn seed_stem_violation(h: &Harness) -> (i64, i64, i64) {
    let adapt = seed_word(&h.store, "adapt", Role::Target, Some(1520)).await;
    let adapter = seed_word(&h.store, "adapter", Role::Target, Some(6000)).await;
    let adopt = seed_word(&h.store, "adopt", Role::Target, Some(1385)).await;
    seed_word(&h.store, "adept", Role::Target, Some(4890)).await;
    set_core_ready(&h.store, &[adapt, adapter, adopt]).await;
    h.store
        .write(
            Actor::Reconciler,
            WriteOp::BindDistractors(BindDistractors {
                bindings: vec![DistractorBinding {
                    word_id: adapt,
                    ranks: vec![(1, adapter)],
                }],
                algo_ver: "distractor/2".into(),
            }),
        )
        .await
        .unwrap();
    (adapt, adapter, adopt)
}

async fn bound_at_rank(store: &Store, word_id: i64, rank: i64) -> i64 {
    store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT distractor_word_id FROM distractors WHERE word_id = ?1 AND rank = ?2",
                rusqlite::params![word_id, rank],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn a_dry_run_reports_the_plan_and_changes_nothing() {
    let h = harness();
    let (adapt, adapter, adopt) = seed_stem_violation(&h).await;

    let (status, body) = post(&h.router, REBIND, serde_json::json!({"dry_run": true})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["scanned"], 1);
    assert_eq!(body["violations"], 1);
    assert_eq!(body["applied"], false);
    assert_eq!(body["truncated"], false);
    assert_eq!(body["skipped"], 0);
    assert_eq!(body["unresolvable_total"], 0);
    assert_eq!(body["planned_or_applied_total"], 1);

    let item = &body["planned_or_applied"][0];
    assert_eq!(item["word_id"], adapt);
    assert_eq!(item["lemma"], "adapt");
    assert_eq!(item["rank"], 1);
    assert_eq!(item["old"]["word_id"], adapter);
    assert_eq!(item["old"]["lemma"], "adapter");
    assert_eq!(item["new"]["word_id"], adopt);
    assert_eq!(item["new"]["lemma"], "adopt");
    assert_eq!(item["core_ready_new"], true);

    assert_eq!(
        bound_at_rank(&h.store, adapt, 1).await,
        adapter,
        "a dry run must not touch the table"
    );
}

#[tokio::test]
async fn a_bodyless_call_is_a_dry_run() {
    let h = harness();
    let (adapt, adapter, _) = seed_stem_violation(&h).await;

    let (status, body) = post(&h.router, REBIND, serde_json::Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["applied"], false);
    assert_eq!(bound_at_rank(&h.store, adapt, 1).await, adapter);
}

#[tokio::test]
async fn applying_moves_the_binding_and_audits_it() {
    let h = harness();
    let (adapt, adapter, adopt) = seed_stem_violation(&h).await;

    let (status, body) = post(&h.router, REBIND, serde_json::json!({"dry_run": false})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["applied"], true);
    assert_eq!(body["violations"], 1);
    assert_eq!(body["skipped"], 0);
    assert_eq!(body["planned_or_applied"][0]["new"]["word_id"], adopt);

    assert_eq!(bound_at_rank(&h.store, adapt, 1).await, adopt);

    let detail = h
        .store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT detail FROM events
                 WHERE entity_type = 'distractor' AND action = 'distractor_bound'
                 ORDER BY event_id DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap();
    let detail: serde_json::Value = serde_json::from_str(&detail).unwrap();
    assert_eq!(detail["reason"], "stem_violation_rebind");
    assert_eq!(detail["old_distractor_word_id"], adapter);
    assert_eq!(detail["new_distractor_word_id"], adopt);

    // The repair is complete: a second run finds nothing left to do.
    let (status, body) = post(&h.router, REBIND, serde_json::json!({"dry_run": false})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["violations"], 0);
    assert_eq!(body["planned_or_applied"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn a_violation_with_no_shippable_replacement_keeps_its_row() {
    let h = harness();
    let adapt = seed_word(&h.store, "adapt", Role::Target, Some(1520)).await;
    let adapter = seed_word(&h.store, "adapter", Role::Target, Some(6000)).await;
    // `adopt` is one edit away but has never been made shippable.
    seed_word(&h.store, "adopt", Role::Target, Some(1385)).await;
    set_core_ready(&h.store, &[adapt, adapter]).await;
    h.store
        .write(
            Actor::Reconciler,
            WriteOp::BindDistractors(BindDistractors {
                bindings: vec![DistractorBinding {
                    word_id: adapt,
                    ranks: vec![(1, adapter)],
                }],
                algo_ver: "distractor/2".into(),
            }),
        )
        .await
        .unwrap();

    let (status, body) = post(&h.router, REBIND, serde_json::json!({"dry_run": false})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["violations"], 1);
    assert_eq!(body["planned_or_applied_total"], 0);
    assert_eq!(body["unresolvable_total"], 1);
    assert_eq!(body["unresolvable"][0]["old"]["word_id"], adapter);
    assert_eq!(body["unresolvable"][0]["new"], serde_json::Value::Null);
    assert_eq!(body["unresolvable"][0]["core_ready_new"], false);
    assert_eq!(bound_at_rank(&h.store, adapt, 1).await, adapter);
}

#[tokio::test]
async fn a_clean_table_reports_no_violations() {
    let h = harness();
    let adapt = seed_word(&h.store, "adapt", Role::Target, Some(1520)).await;
    let adopt = seed_word(&h.store, "adopt", Role::Target, Some(1385)).await;
    set_core_ready(&h.store, &[adapt, adopt]).await;
    h.store
        .write(
            Actor::Reconciler,
            WriteOp::BindDistractors(BindDistractors {
                bindings: vec![DistractorBinding {
                    word_id: adapt,
                    ranks: vec![(1, adopt)],
                }],
                algo_ver: "distractor/2".into(),
            }),
        )
        .await
        .unwrap();

    let (status, body) = post(&h.router, REBIND, serde_json::json!({"dry_run": false})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["scanned"], 1);
    assert_eq!(body["violations"], 0);
    assert_eq!(bound_at_rank(&h.store, adapt, 1).await, adopt);
}
