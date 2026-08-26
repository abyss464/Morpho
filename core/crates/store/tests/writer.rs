//! Integration tests for the single-writer discipline.
//!
//! Every test runs against a temp-file database (WAL needs a real file) and
//! never touches the network.

use std::sync::Arc;

use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, Actor};
use morpho_domain::job::{JobKey, JobKind, JobStatus, RateKey, SubjectRef};
use morpho_domain::types::{
    CandidateKind, CreatedBy, DefinitionSource, ExtractedToken, FetchedImage, ImageSource, Role,
    SelectedBy, SlotRef, WordImport,
};
use morpho_store::ops::{
    CreateWord, IngestImages, MintDefinitionCandidate, OovResolution, RecordDefExtraction,
    SetApproval, SetSelection, UpsertJobState,
};
use morpho_store::{Store, StoreConfig, StoreError, WriteOp};

fn fixture() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(StoreConfig::new(dir.path().join("working.db"))).unwrap();
    (dir, store)
}

fn words(names: &[&str]) -> Vec<WordImport> {
    names
        .iter()
        .map(|w| WordImport {
            word: (*w).to_string(),
            ..Default::default()
        })
        .collect()
}

async fn seed_word(store: &Store, lemma: &str, role: Role) -> i64 {
    store
        .write(
            Actor::Cli,
            WriteOp::CreateWord(CreateWord::new(lemma, role, CreatedBy::Import)),
        )
        .await
        .unwrap()
        .result
        .word_id()
        .unwrap()
}

async fn count(store: &Store, sql: &'static str) -> i64 {
    store
        .read(move |conn| Ok(conn.query_row(sql, [], |row| row.get::<_, i64>(0))?))
        .await
        .unwrap()
}

#[tokio::test]
async fn opens_and_creates_the_schema() {
    let (_dir, store) = fixture();
    assert!(store.was_created());
    assert_eq!(count(&store, "SELECT COUNT(*) FROM words").await, 0);
    assert!(count(&store, "SELECT COUNT(*) FROM rate_limits").await > 0);
}

#[tokio::test]
async fn reopening_keeps_the_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("working.db");
    {
        let store = Store::open(StoreConfig::new(&path)).unwrap();
        assert!(store.was_created());
        store
            .write(
                Actor::Cli,
                WriteOp::import_words(Role::Base, CreatedBy::Import, words(&["kind", "well"])),
            )
            .await
            .unwrap();
    }
    let store = Store::open(StoreConfig::new(&path)).unwrap();
    assert!(!store.was_created());
    assert_eq!(count(&store, "SELECT COUNT(*) FROM words").await, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_write_ops_serialize() {
    let (_dir, store) = fixture();
    let store = Arc::new(store);

    let mut handles = Vec::new();
    for i in 0..32 {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            store
                .write(
                    Actor::System("test".into()),
                    WriteOp::import_words(
                        Role::Target,
                        CreatedBy::Import,
                        vec![WordImport {
                            word: format!("word{i}"),
                            ..Default::default()
                        }],
                    ),
                )
                .await
                .unwrap()
        }));
    }
    let mut inserted = 0;
    for handle in handles {
        inserted += handle
            .await
            .unwrap()
            .result
            .import_stats()
            .unwrap()
            .inserted;
    }
    assert_eq!(inserted, 32);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM words").await, 32);

    // Word ids are dense and unique: no lost update, no duplicate row.
    let distinct = count(&store, "SELECT COUNT(DISTINCT word_id) FROM words").await;
    assert_eq!(distinct, 32);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writes_to_one_slot_produce_a_consistent_revision_chain() {
    let (_dir, store) = fixture();
    let store = Arc::new(store);
    let word_id = seed_word(&store, "benevolent", Role::Target).await;

    let mut cand_ids = Vec::new();
    for i in 0..8 {
        let out = store
            .write(
                Actor::Cli,
                WriteOp::mint_definition(
                    word_id,
                    "adj",
                    format!("candidate number {i}"),
                    DefinitionSource::Freedict,
                ),
            )
            .await
            .unwrap();
        cand_ids.push(out.result.def_cand_id().unwrap());
    }

    let mut handles = Vec::new();
    for cand_id in cand_ids {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            store
                .write(
                    Actor::admin("test"),
                    WriteOp::select(
                        SlotRef::Definition {
                            word_id,
                            pos: "adj".into(),
                        },
                        cand_id,
                        SelectedBy::Human,
                    ),
                )
                .await
                .unwrap()
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }

    // Eight distinct candidates were selected in some serialized order, so the
    // revision counter must have advanced exactly eight times.
    let rev = count(
        &store,
        "SELECT selection_rev FROM definition_selections WHERE word_id = 1 AND pos = 'adj'",
    )
    .await;
    assert_eq!(rev, 8);
    let changes = count(
        &store,
        "SELECT COUNT(*) FROM events WHERE action = 'selection_changed'",
    )
    .await;
    assert_eq!(changes, 8);
}

#[tokio::test]
async fn events_commit_atomically_with_their_cause() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "benevolent", Role::Target).await;

    let outcome = store
        .write(
            Actor::admin("abyss"),
            WriteOp::mint_definition(
                word_id,
                "adj",
                "well meaning and kindly",
                DefinitionSource::Manual,
            ),
        )
        .await
        .unwrap();
    assert_eq!(outcome.events_written, 1);

    let cand_id = outcome.result.def_cand_id().unwrap();
    let rows = store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT actor, action, entity_type FROM events
                 WHERE entity_type = 'definition_candidate' AND entity_id = ?1",
                rusqlite::params![cand_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )?)
        })
        .await
        .unwrap();
    assert_eq!(rows.0, "admin:abyss");
    assert_eq!(rows.1, Action::CandidateAdded.as_str());
    assert_eq!(rows.2, "definition_candidate");
}

#[tokio::test]
async fn a_failing_op_rolls_back_its_event_row() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "benevolent", Role::Target).await;
    let events_before = count(&store, "SELECT COUNT(*) FROM events").await;

    // Batch: a valid mint followed by a mint against a missing word. The whole
    // transaction must vanish, audit row included.
    let err = store
        .write(
            Actor::Cli,
            WriteOp::Batch(vec![
                WriteOp::mint_definition(word_id, "adj", "first", DefinitionSource::Manual),
                WriteOp::mint_definition(9_999, "adj", "second", DefinitionSource::Manual),
            ]),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::NotFound(_)));

    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM events").await,
        events_before
    );
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM definition_candidates").await,
        0
    );
}

#[tokio::test]
async fn batch_composes_an_op_and_a_custom_event_in_one_transaction() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "serene", Role::Target).await;

    let outcome = store
        .write(
            Actor::Reconciler,
            WriteOp::Batch(vec![
                WriteOp::mint_definition(
                    word_id,
                    "adj",
                    "calm and quiet",
                    DefinitionSource::Wordnet,
                ),
                WriteOp::AppendEvent {
                    entity_type: EntityType::Word,
                    entity_id: word_id.to_string(),
                    action: Action::SourceFetched,
                    detail: Some(serde_json::json!({"source": "wordnet"})),
                },
            ]),
        )
        .await
        .unwrap();
    assert_eq!(outcome.events_written, 2);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM events").await, 3); // + word_created
}

#[tokio::test]
async fn change_bus_publishes_after_commit() {
    let (_dir, store) = fixture();
    let mut rx = store.subscribe();

    let word_id = seed_word(&store, "abandon", Role::Target).await;
    let event = rx.try_recv().unwrap();
    assert_eq!(event.entity_type, EntityType::Word);
    assert_eq!(event.entity_ids, vec![word_id.to_string()]);

    store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(word_id, "verb", "to give up", DefinitionSource::Freedict),
        )
        .await
        .unwrap();
    let mut seen = Vec::new();
    while let Ok(event) = rx.try_recv() {
        seen.push(event.entity_type);
    }
    assert!(seen.contains(&EntityType::DefinitionCandidate));
    assert!(seen.contains(&EntityType::Word));
}

#[tokio::test]
async fn import_is_idempotent() {
    let (_dir, store) = fixture();
    let list = vec![
        WordImport {
            word: "  Kind ".into(),
            phonetic: Some("/kaɪnd/".into()),
            frequency_rank: Some(120),
        },
        WordImport {
            word: "well".into(),
            ..Default::default()
        },
        WordImport {
            word: "kind".into(), // duplicate within the same file
            ..Default::default()
        },
        WordImport {
            word: "   ".into(), // blank
            ..Default::default()
        },
    ];

    let first = store
        .write(
            Actor::Cli,
            WriteOp::import_words(Role::Base, CreatedBy::Import, list.clone()),
        )
        .await
        .unwrap();
    let stats = *first.result.import_stats().unwrap();
    assert_eq!(stats.inserted, 2);
    assert_eq!(stats.skipped, 2);

    let second = store
        .write(
            Actor::Cli,
            WriteOp::import_words(Role::Base, CreatedBy::Import, list),
        )
        .await
        .unwrap();
    let stats = *second.result.import_stats().unwrap();
    assert_eq!(stats.inserted, 0);
    assert_eq!(stats.updated, 0);
    assert_eq!(stats.unchanged, 2);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM words").await, 2);
    // A no-op import writes no audit row either.
    assert_eq!(second.events_written, 0);
}

#[tokio::test]
async fn import_never_downgrades_a_role() {
    let (_dir, store) = fixture();
    store
        .write(
            Actor::Cli,
            WriteOp::import_words(Role::Target, CreatedBy::Import, words(&["adapt"])),
        )
        .await
        .unwrap();
    store
        .write(
            Actor::Cli,
            WriteOp::import_words(Role::Base, CreatedBy::Import, words(&["adapt"])),
        )
        .await
        .unwrap();
    let role = store
        .read(|conn| {
            Ok(
                conn.query_row("SELECT role FROM words WHERE lemma = 'adapt'", [], |row| {
                    row.get::<_, String>(0)
                })?,
            )
        })
        .await
        .unwrap();
    assert_eq!(role, "target");
}

#[tokio::test]
async fn import_fills_in_missing_metadata() {
    let (_dir, store) = fixture();
    store
        .write(
            Actor::Cli,
            WriteOp::import_words(Role::Target, CreatedBy::Import, words(&["adept"])),
        )
        .await
        .unwrap();
    let out = store
        .write(
            Actor::Cli,
            WriteOp::import_words(
                Role::Target,
                CreatedBy::Import,
                vec![WordImport {
                    word: "adept".into(),
                    phonetic: Some("/əˈdept/".into()),
                    frequency_rank: Some(3300),
                }],
            ),
        )
        .await
        .unwrap();
    assert_eq!(out.result.import_stats().unwrap().updated, 1);
    let rank = count(
        &store,
        "SELECT frequency_rank FROM words WHERE lemma = 'adept'",
    )
    .await;
    assert_eq!(rank, 3300);
}

#[tokio::test]
async fn minting_the_same_text_twice_dedupes() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "benevolent", Role::Target).await;
    let first = store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(
                word_id,
                "adj",
                "well meaning and kindly",
                DefinitionSource::Freedict,
            ),
        )
        .await
        .unwrap();
    // Same content modulo whitespace: canonicalization must collapse it onto
    // the same text_hash and therefore the same row.
    let second = store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(
                word_id,
                "adj",
                "  well   meaning\tand kindly ",
                DefinitionSource::Freedict,
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        first.result.def_cand_id().unwrap(),
        second.result.def_cand_id().unwrap()
    );
    assert_eq!(second.events_written, 0);
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM definition_candidates").await,
        1
    );
}

#[tokio::test]
async fn first_selected_sense_becomes_primary() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "abandon", Role::Target).await;
    for (pos, text) in [("verb", "to give up"), ("noun", "lack of restraint")] {
        let cand = store
            .write(
                Actor::Cli,
                WriteOp::mint_definition(word_id, pos, text, DefinitionSource::Freedict),
            )
            .await
            .unwrap()
            .result
            .def_cand_id()
            .unwrap();
        store
            .write(
                Actor::Reconciler,
                WriteOp::select(
                    SlotRef::Definition {
                        word_id,
                        pos: pos.into(),
                    },
                    cand,
                    SelectedBy::Auto,
                ),
            )
            .await
            .unwrap();
    }
    let primary_pos = store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT pos FROM definition_selections WHERE is_primary = 1",
                [],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(primary_pos, "verb");
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM definition_selections WHERE is_primary = 1"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn changing_the_selected_candidate_invalidates_approval() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "benevolent", Role::Target).await;
    let slot = SlotRef::Definition {
        word_id,
        pos: "adj".into(),
    };

    let first = store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(word_id, "adj", "kindly", DefinitionSource::Freedict),
        )
        .await
        .unwrap()
        .result
        .def_cand_id()
        .unwrap();
    let second = store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(
                word_id,
                "adj",
                "well meaning and kindly",
                DefinitionSource::Manual,
            ),
        )
        .await
        .unwrap()
        .result
        .def_cand_id()
        .unwrap();

    store
        .write(
            Actor::Reconciler,
            WriteOp::select(slot.clone(), first, SelectedBy::Auto),
        )
        .await
        .unwrap();
    store
        .write(Actor::admin("abyss"), WriteOp::approve(slot.clone()))
        .await
        .unwrap();

    let approved = store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT approved, approved_hash, approved_by, pinned FROM definition_selections",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )?)
        })
        .await
        .unwrap();
    assert_eq!(approved.0, 1);
    assert_eq!(approved.1.unwrap().len(), 64);
    assert_eq!(approved.2.unwrap(), "admin:abyss");
    assert_eq!(approved.3, 1, "approval implies a pin");

    // Move the slot: approval must reset and be audited.
    store
        .write(
            Actor::admin("abyss"),
            WriteOp::select(slot, second, SelectedBy::Human),
        )
        .await
        .unwrap();
    assert_eq!(
        count(
            &store,
            "SELECT approved FROM definition_selections WHERE word_id = 1"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM events WHERE action = 'approval_invalidated'"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn reselecting_the_same_candidate_keeps_approval() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "candid", Role::Target).await;
    let slot = SlotRef::Definition {
        word_id,
        pos: "adj".into(),
    };
    let cand = store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(
                word_id,
                "adj",
                "truthful and open",
                DefinitionSource::Freedict,
            ),
        )
        .await
        .unwrap()
        .result
        .def_cand_id()
        .unwrap();
    store
        .write(
            Actor::Reconciler,
            WriteOp::select(slot.clone(), cand, SelectedBy::Auto),
        )
        .await
        .unwrap();
    store
        .write(Actor::admin("abyss"), WriteOp::approve(slot.clone()))
        .await
        .unwrap();
    store
        .write(
            Actor::admin("abyss"),
            WriteOp::select(slot, cand, SelectedBy::Human),
        )
        .await
        .unwrap();

    let (approved, rev) = store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT approved, selection_rev FROM definition_selections",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(approved, 1);
    assert_eq!(rev, 1, "same content must not bump the revision");
}

#[tokio::test]
async fn rejecting_the_selected_candidate_releases_pin_and_approval() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "obscure", Role::Target).await;
    let slot = SlotRef::Definition {
        word_id,
        pos: "adj".into(),
    };
    let cand = store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(word_id, "adj", "not clear", DefinitionSource::Freedict),
        )
        .await
        .unwrap()
        .result
        .def_cand_id()
        .unwrap();
    store
        .write(
            Actor::Reconciler,
            WriteOp::select(slot.clone(), cand, SelectedBy::Auto),
        )
        .await
        .unwrap();
    store
        .write(Actor::admin("abyss"), WriteOp::approve(slot))
        .await
        .unwrap();

    store
        .write(
            Actor::admin("abyss"),
            WriteOp::RejectCandidate {
                kind: CandidateKind::Definition,
                cand_id: cand,
            },
        )
        .await
        .unwrap();

    let (status, approved, pinned) = store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT dc.status, ds.approved, ds.pinned
                 FROM definition_selections ds
                 JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )?)
        })
        .await
        .unwrap();
    assert_eq!(status, "rejected");
    assert_eq!(approved, 0);
    assert_eq!(pinned, 0);
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM events WHERE action = 'pin_fallback'"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn selecting_a_rejected_candidate_is_a_conflict() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "lucid", Role::Target).await;
    let cand = store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(
                word_id,
                "adj",
                "easy to understand",
                DefinitionSource::Wordnet,
            ),
        )
        .await
        .unwrap()
        .result
        .def_cand_id()
        .unwrap();
    store
        .write(
            Actor::Cli,
            WriteOp::RejectCandidate {
                kind: CandidateKind::Definition,
                cand_id: cand,
            },
        )
        .await
        .unwrap();
    let err = store
        .write(
            Actor::Cli,
            WriteOp::select(
                SlotRef::Definition {
                    word_id,
                    pos: "adj".into(),
                },
                cand,
                SelectedBy::Human,
            ),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::Conflict(_)));
}

#[tokio::test]
async fn extraction_is_discarded_when_the_input_drifted() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "benevolent", Role::Target).await;
    let cand = store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(
                word_id,
                "adj",
                "well meaning and kindly",
                DefinitionSource::Freedict,
            ),
        )
        .await
        .unwrap()
        .result
        .def_cand_id()
        .unwrap();

    let out = store
        .write(
            Actor::Worker(JobKind::ExtractTokens),
            WriteOp::RecordDefExtraction(RecordDefExtraction {
                def_cand_id: cand,
                expected_text_hash: "0".repeat(64),
                input_hash: "1".repeat(64),
                tokenizer_ver: "t/1".into(),
                lemmatizer_ver: "l/1".into(),
                tokens: vec![ExtractedToken {
                    position: 0,
                    surface: "well".into(),
                    lemma: "well".into(),
                }],
            }),
        )
        .await
        .unwrap();
    assert!(matches!(
        out.result,
        morpho_store::WriteResult::Extraction { applied: false }
    ));
    assert_eq!(count(&store, "SELECT COUNT(*) FROM def_tokens").await, 0);
}

#[tokio::test]
async fn oov_promote_creates_an_active_auxiliary_and_closes_the_row() {
    let (_dir, store) = fixture();
    let out = store
        .write(
            Actor::admin("abyss"),
            WriteOp::ResolveOov {
                lemma: "Serene".into(),
                resolution: OovResolution::Promote {
                    phonetic: None,
                    frequency_rank: None,
                },
            },
        )
        .await
        .unwrap();
    assert!(out.result.word_id().is_some());

    let (role, aux, created_by) = store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT role, aux_status, created_by FROM words WHERE lemma = 'serene'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )?)
        })
        .await
        .unwrap();
    assert_eq!(role, "auxiliary");
    assert_eq!(aux, "active");
    assert_eq!(created_by, "promotion");

    let status = store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT status FROM oos_queue WHERE oos_lemma = 'serene'",
                [],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(status, "resolved_promote");
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM events WHERE action = 'aux_promoted'"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn oov_rewrite_mints_a_child_candidate_and_selects_it() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "benevolent", Role::Target).await;
    let parent = store
        .write(
            Actor::Cli,
            WriteOp::mint_definition(
                word_id,
                "adj",
                "altruistic and kindly",
                DefinitionSource::Freedict,
            ),
        )
        .await
        .unwrap()
        .result
        .def_cand_id()
        .unwrap();

    let out = store
        .write(
            Actor::admin("abyss"),
            WriteOp::ResolveOov {
                lemma: "altruistic".into(),
                resolution: OovResolution::Rewrite {
                    def_cand_id: parent,
                    text: "well meaning and kindly".into(),
                    source: DefinitionSource::Manual,
                },
            },
        )
        .await
        .unwrap();
    let child = out.result.def_cand_id().unwrap();
    assert_ne!(child, parent);

    let (selected, parent_link) = store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT ds.def_cand_id, dc.parent_cand_id
                 FROM definition_selections ds
                 JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(selected, child);
    assert_eq!(parent_link, Some(parent));
}

#[tokio::test]
async fn job_state_records_failures_and_clears_on_success() {
    let (_dir, store) = fixture();
    let key = JobKey::new(JobKind::FetchDefinitions, SubjectRef::word(1));

    store
        .write(
            Actor::Reconciler,
            WriteOp::job_backoff(
                key.clone(),
                RateKey::Freedict,
                1,
                "2026-08-26T00:00:00.000Z".into(),
                "timeout".into(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(count(&store, "SELECT COUNT(*) FROM job_state").await, 1);
    // Backoff churn stays out of the audit log.
    assert_eq!(count(&store, "SELECT COUNT(*) FROM events").await, 0);

    let dead = store
        .write(
            Actor::Reconciler,
            WriteOp::UpsertJobState(UpsertJobState {
                key: key.clone(),
                rate_key: RateKey::Freedict,
                status: JobStatus::Dead,
                attempts: 8,
                next_retry_at: None,
                last_error: Some("gave up".into()),
            }),
        )
        .await
        .unwrap();
    assert_eq!(dead.events_written, 1);

    store
        .write(Actor::Reconciler, WriteOp::ClearJobState { key })
        .await
        .unwrap();
    assert_eq!(count(&store, "SELECT COUNT(*) FROM job_state").await, 0);
}

#[tokio::test]
async fn source_fetch_marker_records_zero_results() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "assiduous", Role::Target).await;
    for _ in 0..2 {
        store
            .write(
                Actor::Worker(JobKind::FetchDefinitions),
                WriteOp::RecordSourceFetch {
                    kind: "definitions".into(),
                    word_id,
                    source: "freedict".into(),
                    result_count: 0,
                },
            )
            .await
            .unwrap();
    }
    assert_eq!(count(&store, "SELECT COUNT(*) FROM source_fetch").await, 1);
    assert_eq!(
        count(&store, "SELECT result_count FROM source_fetch").await,
        0
    );
}

/// A second pass over a provider marks itself under its own name, and an empty
/// answer is still an answer: the mark lands, the strict one is untouched, and
/// the chain that reads it can move on.
#[tokio::test]
async fn a_second_pass_marks_itself_even_when_it_finds_nothing() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "manner", Role::Target).await;

    let ingest = |source: ImageSource, mark: Option<&str>, images: Vec<FetchedImage>| {
        WriteOp::IngestImages(IngestImages {
            word_id,
            source,
            images,
            media: Vec::new(),
            mark_source: mark.map(str::to_string),
        })
    };

    store
        .write(
            Actor::Worker(JobKind::FetchImages),
            ingest(ImageSource::Openverse, None, Vec::new()),
        )
        .await
        .unwrap();
    store
        .write(
            Actor::Worker(JobKind::FetchImages),
            ingest(
                ImageSource::Openverse,
                Some("openverse_relaxed"),
                Vec::new(),
            ),
        )
        .await
        .unwrap();

    let marks = store
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT source, result_count FROM source_fetch
                 WHERE kind = 'images' ORDER BY source",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
        .unwrap();
    assert_eq!(
        marks,
        vec![
            ("openverse".to_string(), 0),
            ("openverse_relaxed".to_string(), 0)
        ]
    );
}

/// The mark is free text; `image_candidates.source` is not. A second pass
/// records the provider the picture really came from and marks itself
/// separately — anything else would either break the `CHECK` or lie about where
/// the bytes are from.
#[tokio::test]
async fn a_second_pass_candidate_still_names_its_provider() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "desire", Role::Target).await;
    let dir = tempfile::tempdir().unwrap();
    let media = morpho_store::MediaStore::new(dir.path());
    let stored = media
        .put_bytes(b"RIFF-pretend-webp", morpho_domain::types::MediaKind::Image)
        .unwrap();

    store
        .write(
            Actor::Worker(JobKind::FetchImages),
            WriteOp::IngestImages(IngestImages {
                word_id,
                source: ImageSource::Openverse,
                images: vec![FetchedImage {
                    file_hash: stored.file_hash.clone(),
                    width: Some(1024),
                    height: Some(768),
                    source: ImageSource::Openverse,
                    source_ref: Some("openverse:abc (relaxed-license)".into()),
                    license: Some("CC BY-NC 4.0; by Ada (Openverse)".into()),
                    query_used: Some("desire".into()),
                }],
                media: vec![morpho_store::ops::MediaRegistration {
                    file_hash: stored.file_hash,
                    kind: morpho_domain::types::MediaKind::Image,
                    rel_path: stored.rel_path,
                    bytes: stored.bytes,
                }],
                mark_source: Some("openverse_relaxed".into()),
            }),
        )
        .await
        .unwrap();

    let (source, source_ref, license) = store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT source, source_ref, license FROM image_candidates",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )?)
        })
        .await
        .unwrap();
    assert_eq!(source, "openverse");
    assert_eq!(source_ref, "openverse:abc (relaxed-license)");
    assert_eq!(license, "CC BY-NC 4.0; by Ada (Openverse)");
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM source_fetch WHERE source = 'openverse_relaxed'"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn approve_without_a_selection_is_not_found() {
    let (_dir, store) = fixture();
    let word_id = seed_word(&store, "prudent", Role::Target).await;
    let err = store
        .write(
            Actor::admin("abyss"),
            WriteOp::SetApproval(SetApproval {
                slot: SlotRef::Definition {
                    word_id,
                    pos: "adj".into(),
                },
                approved: true,
            }),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::NotFound(_)));
}

#[tokio::test]
async fn selecting_another_words_candidate_is_a_conflict() {
    let (_dir, store) = fixture();
    let a = seed_word(&store, "adapt", Role::Target).await;
    let b = seed_word(&store, "adopt", Role::Target).await;
    let cand = store
        .write(
            Actor::Cli,
            WriteOp::MintDefinitionCandidate(MintDefinitionCandidate {
                word_id: a,
                pos: "verb".into(),
                text: "to change to fit a new situation".into(),
                source: DefinitionSource::Freedict,
                source_ref: None,
                parent_cand_id: None,
                created_by: None,
                select: false,
            }),
        )
        .await
        .unwrap()
        .result
        .def_cand_id()
        .unwrap();

    let err = store
        .write(
            Actor::Cli,
            WriteOp::SetSelection(SetSelection {
                slot: SlotRef::Definition {
                    word_id: b,
                    pos: "verb".into(),
                },
                cand_id: cand,
                selected_by: SelectedBy::Human,
                pinned: true,
            }),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::Conflict(_)));
}
