//! Chinese gloss anchors in the reconciler (admin-api.md ruling #18a).
//!
//! A word that definitions keep reaching for but that nobody is ever going to
//! learn can carry a short Chinese gloss. That gloss terminates the readability
//! chain exactly like a base word: the dependent stops reporting the token as
//! out of scope, and the anchor itself drops out of the factory — no fetches,
//! no plan slot, no readiness verdict — while staying alive as long as
//! something still points at it.

mod common;

use std::sync::Arc;

use common::{converge, derive, harness, job_subjects, local_reconciler, seed_word, Harness};
use morpho_domain::event::Actor;
use morpho_domain::types::{GlossSource, Role, SelectedBy, SlotRef};
use morpho_reconcile::rules::{FetchDefinitionsRule, FetchExamplesRule};
use morpho_reconcile::sources::SourceSet;
use morpho_reconcile::{AdapterConfig, EngineContext, JobSpec, Rule, SourcesConfig};
use morpho_store::ops::{OovResolution, SetGloss};
use morpho_store::{MediaStore, Store, WriteOp};

const DEFINITION: &str = "to alter a plan";
/// The one lemma in `DEFINITION` that no seeded word covers.
const OUT_OF_SCOPE: &str = "alter";

fn engine(fixture: &Harness) -> Arc<EngineContext> {
    let sources =
        SourceSet::load(SourcesConfig::default(), AdapterConfig::default()).expect("source set");
    Arc::new(EngineContext::new(
        sources,
        MediaStore::new(fixture.dir.path()),
    ))
}

/// One target word whose selected definition leans on an out-of-scope lemma.
async fn dependent_word(fixture: &Harness) -> i64 {
    let store = &fixture.store;
    for lemma in ["to", "a", "plan"] {
        seed_word(store, lemma, Role::Base).await;
    }
    let word_id = seed_word(store, "adapt", Role::Target).await;
    let cand = common::seed_definition(store, word_id, "verb", DEFINITION).await;
    store
        .write(
            Actor::Cli,
            WriteOp::select(
                SlotRef::Definition {
                    word_id,
                    pos: "verb".to_string(),
                },
                cand,
                SelectedBy::Auto,
            ),
        )
        .await
        .unwrap();
    store
        .write(
            Actor::Cli,
            WriteOp::SetPrimarySense {
                word_id,
                pos: "verb".to_string(),
            },
        )
        .await
        .unwrap();
    word_id
}

async fn blockers(store: &Store, word_id: i64) -> Vec<String> {
    store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT blockers FROM words WHERE word_id = ?1",
                rusqlite::params![word_id],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .map(|raw| morpho_domain::blocker::parse_blockers(&raw))
        .unwrap()
}

async fn word_id_of(store: &Store, lemma: &'static str) -> i64 {
    store
        .read(move |conn| morpho_store::queries::word_id_by_lemma(conn, lemma))
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("no word row for {lemma}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_glossed_dependency_stops_being_out_of_scope() {
    let fixture = harness();
    let word_id = dependent_word(&fixture).await;
    let reconciler = local_reconciler(&fixture);
    converge(&reconciler).await;

    // Nothing covers `alter` yet, so the dependent is honestly blocked.
    assert!(
        blockers(&fixture.store, word_id)
            .await
            .contains(&"oos_pending".to_string()),
        "an uncovered token must block"
    );

    // Anchoring the lemma is the third way to close the queue entry.
    fixture
        .store
        .write(
            Actor::admin("abyss"),
            WriteOp::ResolveOov {
                lemma: OUT_OF_SCOPE.to_string(),
                resolution: OovResolution::Gloss {
                    zh_gloss: "改变".to_string(),
                    source: GlossSource::Manual,
                },
            },
        )
        .await
        .unwrap();
    converge(&reconciler).await;

    let after = blockers(&fixture.store, word_id).await;
    assert!(
        !after.contains(&"oos_pending".to_string()),
        "a glossed dependency is satisfied: {after:?}"
    );
    assert!(
        !after.contains(&"dependency_not_ready".to_string()),
        "and it is not a plan-ordering problem either: {after:?}"
    );

    let status: String = fixture
        .store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT status FROM oos_queue WHERE oos_lemma = 'alter'",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(status, "resolved_gloss");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gloss_anchor_asks_for_no_assets() {
    let fixture = harness();
    let word_id = dependent_word(&fixture).await;
    let anchor = seed_word(&fixture.store, OUT_OF_SCOPE, Role::Auxiliary).await;
    let context = engine(&fixture);

    // The word-scoped fetch rules fan out over `word_id`; TTS is keyed by the
    // content address of the text, so it is checked through the desired set.
    let rules: Vec<Arc<dyn Rule>> = vec![
        Arc::new(FetchDefinitionsRule::new(context.clone())),
        Arc::new(FetchExamplesRule::new(context.clone())),
    ];
    for rule in &rules {
        assert!(
            touches(&derive(&fixture.store, rule.clone()).await, anchor),
            "{} works on an ordinary auxiliary",
            rule.name()
        );
    }
    assert!(desired_tts_texts(&fixture.store)
        .await
        .contains(&OUT_OF_SCOPE.to_string()));

    fixture
        .store
        .write(
            Actor::admin("abyss"),
            WriteOp::SetGloss(SetGloss {
                word_id: anchor,
                zh_gloss: Some("改变".to_string()),
                source: GlossSource::Manual,
            }),
        )
        .await
        .unwrap();

    for rule in &rules {
        assert!(
            !touches(&derive(&fixture.store, rule.clone()).await, anchor),
            "{} still wants work for the anchor",
            rule.name()
        );
    }
    let spoken = desired_tts_texts(&fixture.store).await;
    assert!(
        !spoken.contains(&OUT_OF_SCOPE.to_string()),
        "an anchor is read in Chinese, never spoken: {spoken:?}"
    );

    // The word it grounds is still very much the factory's business.
    assert!(touches(
        &derive(&fixture.store, rules[0].clone()).await,
        word_id
    ));
}

/// Does any derived job fan out over this word? Fetch subjects are
/// `word_id:source`, so the id is the part before the colon.
fn touches(jobs: &[JobSpec], word_id: i64) -> bool {
    jobs.iter().any(|job| {
        job.key.subject.subject_id.split(':').next() == Some(word_id.to_string().as_str())
    })
}

async fn desired_tts_texts(store: &Store) -> Vec<String> {
    store
        .read(morpho_store::queries::tts_desired)
        .await
        .unwrap()
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gloss_anchor_takes_no_plan_slot_and_carries_no_blockers() {
    let fixture = harness();
    dependent_word(&fixture).await;
    let reconciler = local_reconciler(&fixture);
    converge(&reconciler).await;

    let anchor = seed_word(&fixture.store, OUT_OF_SCOPE, Role::Auxiliary).await;
    converge(&reconciler).await;
    assert!(
        !blockers(&fixture.store, anchor).await.is_empty(),
        "an ordinary auxiliary is graded like any other word"
    );

    fixture
        .store
        .write(Actor::admin("abyss"), WriteOp::set_gloss(anchor, "改变"))
        .await
        .unwrap();
    converge(&reconciler).await;

    assert!(
        blockers(&fixture.store, anchor).await.is_empty(),
        "an anchor has no gates left to fail"
    );
    let placed = common::scalar_i64(
        &fixture.store,
        "SELECT COUNT(*) FROM plan_words pw
         JOIN plan_artifacts pa ON pa.plan_id = pw.plan_id AND pa.is_current = 1
         JOIN words w ON w.word_id = pw.word_id
         WHERE w.zh_gloss IS NOT NULL",
    )
    .await;
    assert_eq!(placed, 0, "an anchor is not on the curriculum");
    // And the word that leans on it still is.
    let dependent_placed = common::scalar_i64(
        &fixture.store,
        "SELECT COUNT(*) FROM plan_words pw
         JOIN plan_artifacts pa ON pa.plan_id = pw.plan_id AND pa.is_current = 1",
    )
    .await;
    assert!(dependent_placed >= 1, "the dependent kept its slot");
}

/// Ruling #18a's liveness clause: an anchor that a selected definition still
/// mentions counts as referenced, so the retirement sweep leaves it alone even
/// though it has no assets and no plan slot.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_referenced_gloss_anchor_stays_live() {
    let fixture = harness();
    dependent_word(&fixture).await;
    let reconciler = local_reconciler(&fixture);
    converge(&reconciler).await;

    fixture
        .store
        .write(
            Actor::admin("abyss"),
            WriteOp::ResolveOov {
                lemma: OUT_OF_SCOPE.to_string(),
                resolution: OovResolution::Gloss {
                    zh_gloss: "改变".to_string(),
                    source: GlossSource::Manual,
                },
            },
        )
        .await
        .unwrap();
    converge(&reconciler).await;

    let anchor = word_id_of(&fixture.store, OUT_OF_SCOPE).await;
    let (status, live): (String, i64) = fixture
        .store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT w.aux_status, l.is_live FROM words w
                 JOIN aux_liveness l ON l.word_id = w.word_id
                 WHERE w.word_id = ?1",
                rusqlite::params![anchor],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(status, "active");
    assert_eq!(live, 1, "a referenced anchor is live");

    // Take the reference away and it retires like any other auxiliary.
    fixture
        .store
        .write(
            Actor::admin("abyss"),
            WriteOp::SetSlotEnabled {
                word_id: word_id_of(&fixture.store, "adapt").await,
                pos: "verb".to_string(),
                enabled: false,
            },
        )
        .await
        .unwrap();
    converge(&reconciler).await;
    let status: String = fixture
        .store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT aux_status FROM words WHERE word_id = ?1",
                rusqlite::params![anchor],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(status, "retired");
}

/// Clearing the gloss hands the word back to the factory exactly as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clearing_the_gloss_resumes_normal_life() {
    let fixture = harness();
    dependent_word(&fixture).await;
    let anchor = seed_word(&fixture.store, OUT_OF_SCOPE, Role::Auxiliary).await;
    let reconciler = local_reconciler(&fixture);

    fixture
        .store
        .write(Actor::admin("abyss"), WriteOp::set_gloss(anchor, "改变"))
        .await
        .unwrap();
    converge(&reconciler).await;
    assert!(blockers(&fixture.store, anchor).await.is_empty());

    fixture
        .store
        .write(Actor::admin("abyss"), WriteOp::clear_gloss(anchor))
        .await
        .unwrap();
    converge(&reconciler).await;

    let after = blockers(&fixture.store, anchor).await;
    assert!(
        after.contains(&"missing_definition".to_string()),
        "the word is graded again: {after:?}"
    );
    let jobs = job_subjects(
        &derive(
            &fixture.store,
            Arc::new(FetchDefinitionsRule::new(engine(&fixture))) as Arc<dyn Rule>,
        )
        .await,
    );
    assert!(
        jobs.iter().any(|job| job.contains(&anchor.to_string())),
        "and the factory wants its assets again: {jobs:?}"
    );
}
