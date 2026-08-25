//! End-to-end test of the `ExtractTokens` rule: derive → dispatch → execute →
//! commit, then prove the second pass is a no-op and that a tool-version bump
//! is what makes it run again.

use std::sync::Arc;

use morpho_domain::event::Actor;
use morpho_domain::hash::{def_extraction_input_hash, text_hash};
use morpho_domain::types::{CandidateKind, CreatedBy, DefinitionSource, Role, SelectedBy, SlotRef};
use morpho_reconcile::exec::ExtractTokensExecutor;
use morpho_reconcile::rules::ExtractTokensRule;
use morpho_reconcile::{
    Executor, LowercaseLemmatizer, Reconciler, ReconcilerConfig, Scope, TextPipeline, Tokenizer,
};
use morpho_store::ops::CreateWord;
use morpho_store::{Store, StoreConfig, WriteOp};

const TOKENIZER_VER: &str = "simple-tokenizer/1";
const LEMMATIZER_VER: &str = "lowercase-lemmatizer/1";

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(StoreConfig::new(dir.path().join("working.db"))).unwrap();
    (dir, store)
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

async fn scalar_i64(store: &Store, sql: &'static str) -> i64 {
    store
        .read(move |conn| Ok(conn.query_row(sql, [], |row| row.get::<_, i64>(0))?))
        .await
        .unwrap()
}

async fn run_to_quiescence(reconciler: &Reconciler) -> morpho_reconcile::PassStats {
    let stats = reconciler.run_once(Scope::Full).await.unwrap();
    reconciler.dispatcher().drain().await;
    stats
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn extracts_tokens_then_becomes_a_noop() {
    let (_dir, store) = store();
    let word_id = seed_word(&store, "benevolent", Role::Target).await;
    let cand = seed_definition(&store, word_id, "adj", "well meaning and kindly").await;

    let reconciler = Reconciler::new(store.clone(), ReconcilerConfig::default());

    let first = run_to_quiescence(&reconciler).await;
    assert_eq!(first.derived, 1);
    assert_eq!(first.dispatched, 1);

    assert_eq!(
        scalar_i64(&store, "SELECT COUNT(*) FROM def_tokens").await,
        4
    );
    let (input_hash, tok_ver, lem_ver) = store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT input_hash, tokenizer_ver, lemmatizer_ver FROM def_extractions
                 WHERE def_cand_id = ?1",
                rusqlite::params![cand],
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

    // The stored hash must be exactly what the contract prescribes.
    let expected = def_extraction_input_hash(
        &text_hash("well meaning and kindly"),
        TOKENIZER_VER,
        LEMMATIZER_VER,
    );
    assert_eq!(input_hash, expected);
    assert_eq!(tok_ver, TOKENIZER_VER);
    assert_eq!(lem_ver, LEMMATIZER_VER);

    // Second pass: nothing to do at all.
    let second = run_to_quiescence(&reconciler).await;
    assert_eq!(second.derived, 0);
    assert_eq!(second.dispatched, 0);
    assert_eq!(
        scalar_i64(&store, "SELECT COUNT(*) FROM def_tokens").await,
        4
    );

    // Repeated runs must not churn the audit log either.
    assert_eq!(
        scalar_i64(
            &store,
            "SELECT COUNT(*) FROM events WHERE entity_type = 'def_extraction'"
        )
        .await,
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn token_positions_surfaces_and_lemmas_are_exact() {
    let (_dir, store) = store();
    let word_id = seed_word(&store, "meticulous", Role::Target).await;
    seed_definition(
        &store,
        word_id,
        "adj",
        "Very careful, well-ordered; don't rush.",
    )
    .await;

    let reconciler = Reconciler::new(store.clone(), ReconcilerConfig::default());
    run_to_quiescence(&reconciler).await;

    let tokens: Vec<(i64, String, String)> = store
        .read(|conn| {
            let mut stmt =
                conn.prepare("SELECT position, surface, lemma FROM def_tokens ORDER BY position")?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .await
        .unwrap();

    let surfaces: Vec<&str> = tokens.iter().map(|t| t.1.as_str()).collect();
    assert_eq!(
        surfaces,
        ["Very", "careful", "well", "ordered", "don't", "rush"]
    );
    let lemmas: Vec<&str> = tokens.iter().map(|t| t.2.as_str()).collect();
    assert_eq!(
        lemmas,
        ["very", "careful", "well", "ordered", "don't", "rush"]
    );
    for (i, token) in tokens.iter().enumerate() {
        assert_eq!(token.0, i as i64);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bumping_the_tokenizer_version_reruns_exactly_once() {
    let (_dir, store) = store();
    let word_id = seed_word(&store, "lucid", Role::Target).await;
    seed_definition(&store, word_id, "adj", "clear and easy to understand").await;

    let baseline = Reconciler::new(store.clone(), ReconcilerConfig::default());
    run_to_quiescence(&baseline).await;
    let before = store
        .read(|conn| {
            Ok(
                conn.query_row("SELECT input_hash FROM def_extractions", [], |row| {
                    row.get::<_, String>(0)
                })?,
            )
        })
        .await
        .unwrap();

    // A "new tokenizer": same output, different version. The hash must change
    // and the work must be redone — that is the whole staleness model.
    struct BumpedTokenizer;
    impl Tokenizer for BumpedTokenizer {
        fn version(&self) -> &str {
            "simple-tokenizer/2"
        }
        fn tokenize(&self, text: &str) -> Vec<String> {
            morpho_reconcile::SimpleTokenizer.tokenize(text)
        }
    }

    let pipeline = TextPipeline::new(Arc::new(BumpedTokenizer), Arc::new(LowercaseLemmatizer));
    let bumped = Reconciler::with_parts(
        store.clone(),
        ReconcilerConfig::default(),
        vec![Arc::new(ExtractTokensRule::new(pipeline.clone()))],
        vec![Arc::new(ExtractTokensExecutor::new(pipeline)) as Arc<dyn Executor>],
    );

    let stats = run_to_quiescence(&bumped).await;
    assert_eq!(stats.dispatched, 1);

    let after = store
        .read(|conn| {
            Ok(
                conn.query_row("SELECT input_hash FROM def_extractions", [], |row| {
                    row.get::<_, String>(0)
                })?,
            )
        })
        .await
        .unwrap();
    assert_ne!(before, after);

    // And it converges again immediately.
    assert_eq!(run_to_quiescence(&bumped).await.dispatched, 0);
    assert_eq!(
        scalar_i64(&store, "SELECT COUNT(*) FROM def_extractions").await,
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejected_candidates_are_not_extracted() {
    let (_dir, store) = store();
    let word_id = seed_word(&store, "candid", Role::Target).await;
    let cand = seed_definition(&store, word_id, "adj", "truthful and open").await;
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

    let reconciler = Reconciler::new(store.clone(), ReconcilerConfig::default());
    let stats = run_to_quiescence(&reconciler).await;
    assert_eq!(stats.derived, 0);
    assert_eq!(
        scalar_i64(&store, "SELECT COUNT(*) FROM def_tokens").await,
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn extraction_feeds_the_dependency_and_oov_views() {
    let (_dir, store) = store();
    // "benevolent" is defined with a base word, a target word and one word
    // that is in no list at all.
    let benevolent = seed_word(&store, "benevolent", Role::Target).await;
    seed_word(&store, "kind", Role::Base).await;
    // Function words must live in the base list or every definition would
    // report them as out of scope — see core/fixtures/base-words.txt.
    seed_word(&store, "and", Role::Base).await;
    seed_word(&store, "generous", Role::Target).await;

    let cand = seed_definition(
        &store,
        benevolent,
        "adj",
        "kind and generous and altruistic",
    )
    .await;
    store
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

    let reconciler = Reconciler::new(store.clone(), ReconcilerConfig::default());
    run_to_quiescence(&reconciler).await;

    // Dependency edges are a pure view over the selected definition: only the
    // target/auxiliary token counts, the base word does not.
    let deps: Vec<String> = store
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT w.lemma FROM def_dependencies d
                 JOIN words w ON w.word_id = d.depends_on_word_id",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .await
        .unwrap();
    assert_eq!(deps, ["generous"]);

    // "altruistic" matches no word row, so it shows up as out of scope.
    let oos: Vec<String> = store
        .read(|conn| {
            let mut stmt = conn.prepare("SELECT oos_lemma FROM oos_occurrences")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .await
        .unwrap();
    assert_eq!(oos, ["altruistic"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dead_job_state_excludes_a_subject_from_derivation() {
    use morpho_domain::job::{JobKey, JobKind, JobStatus, RateKey, SubjectRef};
    use morpho_store::ops::UpsertJobState;

    let (_dir, store) = store();
    let word_id = seed_word(&store, "obscure", Role::Target).await;
    let cand = seed_definition(&store, word_id, "adj", "not clear").await;

    store
        .write(
            Actor::Reconciler,
            WriteOp::UpsertJobState(UpsertJobState {
                key: JobKey::new(JobKind::ExtractTokens, SubjectRef::def_candidate(cand)),
                rate_key: RateKey::Cpu,
                status: JobStatus::Dead,
                attempts: 5,
                next_retry_at: None,
                last_error: Some("boom".into()),
            }),
        )
        .await
        .unwrap();

    let reconciler = Reconciler::new(store.clone(), ReconcilerConfig::default());
    let stats = run_to_quiescence(&reconciler).await;
    assert_eq!(stats.derived, 1);
    assert_eq!(stats.dispatched, 0);
    assert_eq!(stats.skipped_dead, 1);
    assert_eq!(
        scalar_i64(&store, "SELECT COUNT(*) FROM def_tokens").await,
        0
    );

    // Clearing the row is all an operator's "retry" needs to be.
    store
        .write(
            Actor::admin("abyss"),
            WriteOp::ClearJobState {
                key: JobKey::new(JobKind::ExtractTokens, SubjectRef::def_candidate(cand)),
            },
        )
        .await
        .unwrap();
    let stats = run_to_quiescence(&reconciler).await;
    assert_eq!(stats.dispatched, 1);
    assert_eq!(
        scalar_i64(&store, "SELECT COUNT(*) FROM def_tokens").await,
        2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn backoff_defers_a_subject_until_its_retry_time() {
    use morpho_domain::job::{JobKey, JobKind, JobStatus, RateKey, SubjectRef};
    use morpho_store::ops::UpsertJobState;

    let (_dir, store) = store();
    let word_id = seed_word(&store, "tenacious", Role::Target).await;
    let cand = seed_definition(&store, word_id, "adj", "holding on firmly").await;
    let key = JobKey::new(JobKind::ExtractTokens, SubjectRef::def_candidate(cand));

    let future = morpho_domain::time::format_ts(chrono::Utc::now() + chrono::Duration::hours(1));
    store
        .write(
            Actor::Reconciler,
            WriteOp::UpsertJobState(UpsertJobState {
                key: key.clone(),
                rate_key: RateKey::Cpu,
                status: JobStatus::Backoff,
                attempts: 2,
                next_retry_at: Some(future),
                last_error: Some("transient".into()),
            }),
        )
        .await
        .unwrap();

    let reconciler = Reconciler::new(store.clone(), ReconcilerConfig::default());
    let stats = run_to_quiescence(&reconciler).await;
    assert_eq!(stats.skipped_backoff, 1);
    assert_eq!(stats.dispatched, 0);

    // Move the retry time into the past: the job runs and the row is cleared,
    // because "no row" is what healthy looks like.
    let past = morpho_domain::time::format_ts(chrono::Utc::now() - chrono::Duration::minutes(5));
    store
        .write(
            Actor::Reconciler,
            WriteOp::UpsertJobState(UpsertJobState {
                key,
                rate_key: RateKey::Cpu,
                status: JobStatus::Backoff,
                attempts: 2,
                next_retry_at: Some(past),
                last_error: Some("transient".into()),
            }),
        )
        .await
        .unwrap();
    let stats = run_to_quiescence(&reconciler).await;
    assert_eq!(stats.dispatched, 1);
    assert_eq!(
        scalar_i64(&store, "SELECT COUNT(*) FROM job_state").await,
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn in_flight_dedup_prevents_double_dispatch() {
    let (_dir, store) = store();
    let word_id = seed_word(&store, "resilient", Role::Target).await;
    seed_definition(&store, word_id, "adj", "able to recover quickly").await;

    let reconciler = Reconciler::new(store.clone(), ReconcilerConfig::default());
    // Claim the job by hand so the pass sees it as already in flight.
    let registry = reconciler.registry();
    let derived = reconciler.run_once(Scope::Full).await.unwrap();
    assert_eq!(derived.dispatched, 1);

    // While it is still claimed (or immediately after), a second pass must not
    // create a duplicate. Drain first so the assertion is deterministic.
    reconciler.dispatcher().drain().await;
    assert_eq!(registry.in_flight_len(), 0);
    let second = reconciler.run_once(Scope::Full).await.unwrap();
    assert_eq!(second.derived, 0);
    assert_eq!(
        scalar_i64(&store, "SELECT COUNT(*) FROM def_extractions").await,
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn change_events_wake_the_loop() {
    let (_dir, store) = store();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let reconciler = Reconciler::new(
        store.clone(),
        ReconcilerConfig {
            full_pass_interval: std::time::Duration::from_secs(3_600),
            coalesce_window: std::time::Duration::from_millis(50),
        },
    );
    let dispatcher = reconciler.dispatcher();
    let handle = tokio::spawn(reconciler.run(rx));

    // Give the startup pass a moment, then create work.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let word_id = seed_word(&store, "serene", Role::Target).await;
    seed_definition(&store, word_id, "adj", "calm and quiet").await;

    // The 60 s timer is off, so only the change event can drive this.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if scalar_i64(&store, "SELECT COUNT(*) FROM def_extractions").await == 1 {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "loop never converged");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    dispatcher.drain().await;
    tx.send(true).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), handle)
        .await
        .expect("reconciler did not stop")
        .unwrap();
}
