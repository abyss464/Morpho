//! Admin API integration tests: drive the real router with a real temp
//! database, no network.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use morpho_api::{build_router, AppState};
use morpho_domain::event::Actor;
use morpho_domain::types::{CreatedBy, DefinitionSource, Role, SelectedBy, SlotRef};
use morpho_reconcile::JobRegistry;
use morpho_store::ops::CreateWord;
use morpho_store::{Store, StoreConfig, WriteOp};

struct Harness {
    _dir: tempfile::TempDir,
    store: Store,
    router: axum::Router,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().to_path_buf();
    let store = Store::open(StoreConfig::new(data_dir.join("working.db"))).unwrap();
    let state = AppState::new(store.clone(), Arc::new(JobRegistry::new()), data_dir);
    let router = build_router(state, None);
    Harness {
        _dir: dir,
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

#[tokio::test]
async fn dashboard_reports_real_counts() {
    let h = harness();
    let benevolent = seed_word(&h.store, "benevolent", Role::Target, Some(4312)).await;
    seed_word(&h.store, "kind", Role::Base, None).await;
    let cand = h
        .store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(
                benevolent,
                "adj",
                "well meaning and kind",
                DefinitionSource::Freedict,
            ),
        )
        .await
        .unwrap()
        .result
        .def_cand_id()
        .unwrap();
    h.store
        .write(
            Actor::Reconciler,
            WriteOp::select(
                SlotRef::Definition {
                    word_id: benevolent,
                    pos: "adj".into(),
                },
                cand,
                SelectedBy::Auto,
            ),
        )
        .await
        .unwrap();

    let (status, body) = get(&h.router, "/api/dashboard").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["words"]["total"], 2);
    assert_eq!(body["words"]["target"], 1);
    assert_eq!(body["words"]["auxiliary"], 0);
    assert_eq!(body["words"]["ready"], 0);
    assert_eq!(body["words"]["blocked"], 1);
    assert_eq!(body["assets"]["definitions"], 1);
    // The word lemma and its selected definition both need speaking.
    assert_eq!(body["assets"]["tts"]["missing"], 2);
    assert_eq!(body["oos_open"], 0);
    assert_eq!(body["dead_letters"], 0);
    assert!(body["plan"].is_null());
    assert!(body["recent_events"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn word_list_paginates_filters_and_rolls_up() {
    let h = harness();
    for (lemma, role, rank) in [
        ("adapt", Role::Target, Some(1520)),
        ("adopt", Role::Target, Some(1385)),
        ("adept", Role::Target, Some(4890)),
        ("kind", Role::Base, None),
    ] {
        seed_word(&h.store, lemma, role, rank).await;
    }

    let (status, body) = get(&h.router, "/api/words").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 4);
    let items = body["items"].as_array().unwrap();
    // Ordered by frequency_rank, unranked last.
    assert_eq!(items[0]["lemma"], "adopt");
    assert_eq!(items[1]["lemma"], "adapt");
    assert_eq!(items[2]["lemma"], "adept");
    assert_eq!(items[3]["lemma"], "kind");
    assert_eq!(items[0]["sense_count"], 0);
    assert_eq!(items[0]["has_image"], false);
    assert_eq!(items[0]["blockers"].as_array().unwrap().len(), 0);

    let (_, body) = get(&h.router, "/api/words?role=target").await;
    assert_eq!(body["total"], 3);

    let (_, body) = get(&h.router, "/api/words?q=ad&page=2&page_size=2").await;
    assert_eq!(body["total"], 3);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);

    let (_, body) = get(&h.router, "/api/words?ready=true").await;
    assert_eq!(body["total"], 0);

    let (_, body) = get(&h.router, "/api/words?blocker=missing_image").await;
    assert_eq!(body["total"], 0);
}

#[tokio::test]
async fn word_detail_includes_slots_and_events() {
    let h = harness();
    let word_id = seed_word(&h.store, "benevolent", Role::Target, Some(4312)).await;
    let (status, created) = post(
        &h.router,
        "/api/candidates/definition",
        serde_json::json!({"word_id": word_id, "pos": "adj", "text": "  well  meaning and kindly "}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    // Text is stored canonicalized.
    assert_eq!(created["text"], "well meaning and kindly");
    assert_eq!(created["source"], "manual");
    let cand_id = created["def_cand_id"].as_i64().unwrap();

    let (status, body) = get(&h.router, &format!("/api/words/{word_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["lemma"], "benevolent");
    assert_eq!(body["phonetic"], serde_json::Value::Null);
    let definitions = body["definitions"].as_array().unwrap();
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0]["pos"], "adj");
    assert!(definitions[0]["selection"].is_null());
    assert_eq!(definitions[0]["candidates"][0]["def_cand_id"], cand_id);
    assert_eq!(body["examples"]["slots"].as_array().unwrap().len(), 0);
    assert!(body["image"]["selection"].is_null());
    assert_eq!(body["distractors"].as_array().unwrap().len(), 0);
    // The word needs its lemma spoken even before any definition is selected.
    assert_eq!(body["tts"][0]["kind"], "word");
    assert_eq!(body["tts"][0]["status"], "missing");
    assert!(!body["recent_events"].as_array().unwrap().is_empty());

    let (status, _) = get(&h.router, "/api/words/9999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn selection_approval_and_unapproval_round_trip() {
    let h = harness();
    let word_id = seed_word(&h.store, "candid", Role::Target, None).await;
    let (_, created) = post(
        &h.router,
        "/api/candidates/definition",
        serde_json::json!({"word_id": word_id, "pos": "adj", "text": "truthful and open"}),
    )
    .await;
    let cand_id = created["def_cand_id"].as_i64().unwrap();

    let (status, detail) = post(
        &h.router,
        "/api/selections/definition",
        serde_json::json!({"word_id": word_id, "pos": "adj", "cand_id": cand_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let selection = &detail["definitions"][0]["selection"];
    assert_eq!(selection["def_cand_id"], cand_id);
    assert_eq!(selection["selected_by"], "human");
    assert_eq!(selection["pinned"], true);
    assert_eq!(selection["is_primary"], true);
    assert_eq!(selection["approved"], false);

    let (status, detail) = post(
        &h.router,
        "/api/selections/definition/approve",
        serde_json::json!({"word_id": word_id, "pos": "adj"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let selection = &detail["definitions"][0]["selection"];
    assert_eq!(selection["approved"], true);
    assert_eq!(selection["approved_by"], "admin:abyss");
    assert_eq!(
        selection["approved_hash"].as_str().unwrap(),
        detail["definitions"][0]["candidates"][0]["text_hash"]
            .as_str()
            .unwrap()
    );

    let (status, detail) = delete(
        &h.router,
        "/api/selections/definition/approve",
        serde_json::json!({"word_id": word_id, "pos": "adj"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["definitions"][0]["selection"]["approved"], false);
}

#[tokio::test]
async fn rejecting_a_selected_candidate_releases_the_slot() {
    let h = harness();
    let word_id = seed_word(&h.store, "obscure", Role::Target, None).await;
    let (_, created) = post(
        &h.router,
        "/api/candidates/definition",
        serde_json::json!({"word_id": word_id, "pos": "adj", "text": "not clear"}),
    )
    .await;
    let cand_id = created["def_cand_id"].as_i64().unwrap();
    post(
        &h.router,
        "/api/selections/definition",
        serde_json::json!({"word_id": word_id, "pos": "adj", "cand_id": cand_id}),
    )
    .await;
    post(
        &h.router,
        "/api/selections/definition/approve",
        serde_json::json!({"word_id": word_id, "pos": "adj"}),
    )
    .await;

    let (status, detail) = post(
        &h.router,
        &format!("/api/candidates/definition/{cand_id}/reject"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        detail["definitions"][0]["candidates"][0]["status"],
        "rejected"
    );
    assert_eq!(detail["definitions"][0]["selection"]["approved"], false);
    assert_eq!(detail["definitions"][0]["selection"]["pinned"], false);

    let (status, _) = post(
        &h.router,
        "/api/candidates/definition/9999/reject",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn oov_resolution_promotes_and_rewrites() {
    let h = harness();
    let (status, body) = post(
        &h.router,
        "/api/oov/Serene/resolve",
        serde_json::json!({"mode": "promote"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["oos_lemma"], "serene");
    assert_eq!(body["status"], "resolved_promote");
    let promoted = body["word_id"].as_i64().unwrap();

    let (_, detail) = get(&h.router, &format!("/api/words/{promoted}")).await;
    assert_eq!(detail["role"], "auxiliary");
    assert_eq!(detail["aux_status"], "active");
    assert_eq!(detail["created_by"], "promotion");

    let word_id = seed_word(&h.store, "benevolent", Role::Target, None).await;
    let (_, created) = post(
        &h.router,
        "/api/candidates/definition",
        serde_json::json!({"word_id": word_id, "pos": "adj", "text": "altruistic and kind"}),
    )
    .await;
    let parent = created["def_cand_id"].as_i64().unwrap();

    let (status, body) = post(
        &h.router,
        "/api/oov/altruistic/resolve",
        serde_json::json!({"mode": "rewrite", "def_cand_id": parent, "text": "well meaning and kind"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "resolved_rewrite");
    let child = body["def_cand_id"].as_i64().unwrap();
    assert_ne!(child, parent);

    let (_, detail) = get(&h.router, &format!("/api/words/{word_id}")).await;
    assert_eq!(detail["definitions"][0]["selection"]["def_cand_id"], child);
}

#[tokio::test]
async fn events_endpoint_filters_and_paginates() {
    let h = harness();
    let word_id = seed_word(&h.store, "prudent", Role::Target, None).await;
    for text in ["careful and wise", "showing care for the future"] {
        post(
            &h.router,
            "/api/candidates/definition",
            serde_json::json!({"word_id": word_id, "pos": "adj", "text": text}),
        )
        .await;
    }

    let (status, body) = get(&h.router, "/api/events").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3); // word_created + 2 candidate_added
                                  // Newest first.
    assert_eq!(body["items"][0]["action"], "candidate_added");
    assert_eq!(body["items"][0]["actor"], "admin:abyss");

    let (_, body) = get(&h.router, "/api/events?entity_type=word").await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["action"], "word_created");
    assert_eq!(body["items"][0]["detail"]["lemma"], "prudent");

    let (_, body) = get(&h.router, "/api/events?page_size=1").await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["total"], 3);
}

#[tokio::test]
async fn jobs_endpoint_reports_lanes_and_backoff() {
    use morpho_domain::job::{JobKey, JobKind, JobStatus, RateKey, SubjectRef};
    use morpho_store::ops::UpsertJobState;

    let h = harness();
    h.store
        .write(
            Actor::Reconciler,
            WriteOp::UpsertJobState(UpsertJobState {
                key: JobKey::new(JobKind::FetchDefinitions, SubjectRef::word(1)),
                rate_key: RateKey::Freedict,
                status: JobStatus::Dead,
                attempts: 8,
                next_retry_at: None,
                last_error: Some("404".into()),
            }),
        )
        .await
        .unwrap();

    let (status, body) = get(&h.router, "/api/jobs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["in_flight"].as_array().unwrap().len(), 0);
    let backoff = body["backoff"].as_array().unwrap();
    assert_eq!(backoff.len(), 1);
    assert_eq!(backoff[0]["kind"], "fetch_definitions");
    assert_eq!(backoff[0]["state"], "dead");
    assert_eq!(backoff[0]["attempts"], 8);
    assert!(body["lanes"].is_object());
}

#[tokio::test]
async fn media_endpoint_serves_registered_bytes() {
    use morpho_domain::hash::file_hash;
    use morpho_domain::types::MediaKind;

    let h = harness();
    let bytes = b"not really a webp, but content-addressed all the same";
    let hash = file_hash(bytes);
    let rel_path = format!("media/{}/{}.webp", &hash[..2], hash);
    let full = h._dir.path().join(&rel_path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(&full, bytes).unwrap();

    h.store
        .write(
            Actor::Cli,
            WriteOp::RegisterMediaFile {
                file_hash: hash.clone(),
                kind: MediaKind::Image,
                rel_path,
                bytes: bytes.len() as i64,
            },
        )
        .await
        .unwrap();

    let response = h
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/media/{hash}"))
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
    let served = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(served.as_ref(), bytes);

    let (status, _) = get(&h.router, &format!("/api/media/{}", "0".repeat(64))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = get(&h.router, "/api/media/short").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn unimplemented_endpoints_answer_501_with_the_error_envelope() {
    let h = harness();
    for (method, uri) in [
        ("POST", "/api/words"),
        ("POST", "/api/candidates/example"),
        ("POST", "/api/candidates/image"),
        ("POST", "/api/selections/definition/primary"),
        ("POST", "/api/selections/definition/enabled"),
        ("GET", "/api/oov"),
        ("GET", "/api/dead-letters"),
        ("POST", "/api/dead-letters/retry"),
        ("POST", "/api/dead-letters/waive"),
        ("GET", "/api/plan"),
        ("GET", "/api/plan/groups/1"),
        ("GET", "/api/releases"),
        ("GET", "/api/releases/preview"),
        ("POST", "/api/releases/export"),
    ] {
        let (status, body) = call(
            &h.router,
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{method} {uri}");
        assert_eq!(body["error"]["code"], "not_implemented", "{method} {uri}");
        assert!(body["error"]["message"].as_str().unwrap().contains(uri));
    }
}

#[tokio::test]
async fn errors_use_the_contract_envelope() {
    let h = harness();
    let (status, body) = post(
        &h.router,
        "/api/candidates/definition",
        serde_json::json!({"word_id": 4242, "pos": "adj", "text": "nope"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    assert!(body["error"]["message"].as_str().unwrap().contains("4242"));

    let word_id = seed_word(&h.store, "frank", Role::Target, None).await;
    let (status, body) = post(
        &h.router,
        "/api/selections/definition",
        serde_json::json!({"word_id": word_id, "cand_id": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");

    let (status, body) = post(
        &h.router,
        "/api/selections/nonsense",
        serde_json::json!({"word_id": word_id, "pos": "adj", "cand_id": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("nonsense"));
}

#[tokio::test]
async fn root_falls_back_to_a_placeholder_without_a_built_ui() {
    let h = harness();
    let response = h
        .router
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("morphod is running"));
}
