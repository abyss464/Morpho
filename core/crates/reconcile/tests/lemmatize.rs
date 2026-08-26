//! Wave 5: real lemmatization end to end.
//!
//! The production symptom this closes: with a lowercase-only "lemmatizer",
//! `having`, `made`, `given`, `known`, `taken` and `relating` never matched
//! `have`, `make`, `give`, `know`, `take` and `relate`, so every one of them
//! opened an OOV entry. Promoting those as auxiliary words would have taught
//! the reader inflections instead of vocabulary.
//!
//! What is asserted here is the whole invalidation story: the version bump
//! alone re-extracts every candidate, exactly once, and the OOV queue shrinks
//! to the words that are genuinely out of scope.

mod common;

use std::sync::Arc;

use common::{
    context, harness, run_to_quiescence, scalar_i64, seed_definition, seed_word, Harness,
};
use morpho_domain::types::Role;
use morpho_reconcile::exec::{Executor, ExtractTokensExecutor};
use morpho_reconcile::rules::ExtractTokensRule;
use morpho_reconcile::{
    ExceptionTable, Lemmatizer, LexiconCache, LowercaseLemmatizer, MorphyLemmatizer, Reconciler,
    ReconcilerConfig, Rule, SimpleTokenizer, TextPipeline,
};
use morpho_store::Store;

/// Vocabulary the reader is assumed to have. Every definition below is written
/// out of it, plus a handful of words that are deliberately missing.
const BASE_WORDS: &[&str] = &[
    "a", "and", "or", "of", "to", "the", "in", "for", "with", "that", "who", "not", "be", "have",
    "make", "give", "know", "take", "relate", "use", "person", "thing", "place", "act", "care",
    "state", "quality", "become", "move", "form", "part", "small", "large", "way", "word", "other",
    "one", "own", "study", "box", "child", "foot", "run", "stop", "easy", "good", "big", "carry",
    "apple", "fruit", "tell",
];

/// Definitions whose tokens are mostly inflections of [`BASE_WORDS`].
const DEFINITIONS: &[(&str, &str, &str)] = &[
    (
        "benevolent",
        "adj",
        "having a kindly quality and given to acts of care",
    ),
    (
        "abandon",
        "verb",
        "to give up a thing or a place that one has taken",
    ),
    (
        "meticulous",
        "adj",
        "relating to careful and precise studies of the smallest parts",
    ),
    (
        "custodian",
        "noun",
        "a person who is known for taking care of a large place",
    ),
    (
        "sprint",
        "verb",
        "to be running at one's fastest, then stopping",
    ),
    (
        "crate",
        "noun",
        "a large box, e.g. one that carries apples and other fruit",
    ),
    (
        "instruct",
        "verb",
        "to tell sb sth in a way that makes it known",
    ),
];

/// The state before wave 5: the wave-5 tokenizer with no morphology at all.
fn legacy_pipeline() -> TextPipeline {
    TextPipeline::new(Arc::new(SimpleTokenizer), Arc::new(LowercaseLemmatizer))
}

fn reconciler_with(fixture: &Harness, pipeline: TextPipeline) -> Reconciler {
    Reconciler::with_parts(
        fixture.store.clone(),
        Arc::new(context(fixture.dir.path()).with_pipeline(pipeline.clone())),
        ReconcilerConfig::default(),
        vec![Arc::new(ExtractTokensRule::new(pipeline.clone())) as Arc<dyn Rule>],
        vec![Arc::new(ExtractTokensExecutor::new(pipeline)) as Arc<dyn Executor>],
    )
}

async fn seed_corpus(store: &Store) {
    for word in BASE_WORDS {
        seed_word(store, word, Role::Base).await;
    }
    for (lemma, pos, text) in DEFINITIONS {
        let word_id = seed_word(store, lemma, Role::Target).await;
        seed_definition(store, word_id, pos, text).await;
    }
}

async fn open_oov(store: &Store) -> Vec<String> {
    store
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT oos_lemma FROM oos_queue WHERE status = 'open' ORDER BY oos_lemma",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
        .unwrap()
}

/// The headline: bumping the tool versions re-extracts everything once, and the
/// inflections leave the OOV queue.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_version_bump_re_extracts_once_and_shrinks_the_oov_queue() {
    let fixture = harness();
    let store = fixture.store.clone();
    seed_corpus(&store).await;

    // -- before: no morphology -------------------------------------------
    let legacy = reconciler_with(&fixture, legacy_pipeline());
    let first = run_to_quiescence(&legacy).await;
    assert_eq!(first.dispatched, DEFINITIONS.len());
    // Let selection and the OOV sweep settle.
    run_to_quiescence(&legacy).await;
    run_to_quiescence(&legacy).await;

    let before = open_oov(&store).await;
    for inflection in ["having", "given", "taken", "relating", "known", "studies"] {
        assert!(
            before.contains(&inflection.to_string()),
            "{inflection} should be OOV before the bump: {before:?}"
        );
    }

    // -- after: morphy ----------------------------------------------------
    let upgraded = reconciler_with(&fixture, TextPipeline::default());
    let bumped = run_to_quiescence(&upgraded).await;
    assert_eq!(
        bumped.dispatched,
        DEFINITIONS.len(),
        "the version bump alone re-extracts every candidate"
    );
    assert_eq!(
        run_to_quiescence(&upgraded).await.dispatched,
        0,
        "and exactly once"
    );
    run_to_quiescence(&upgraded).await;

    let after = open_oov(&store).await;
    for inflection in [
        "having", "given", "taken", "relating", "known", "studies", "acts", "makes", "carries",
        "apples", "stopping", "running", "boxes",
    ] {
        assert!(
            !after.contains(&inflection.to_string()),
            "{inflection} should have resolved to its lemma: {after:?}"
        );
    }
    // Words that really are out of scope stay out of scope.
    for genuine in ["kindly", "precise", "careful", "fastest"] {
        assert!(
            after.contains(&genuine.to_string()),
            "{genuine} is genuinely unknown: {after:?}"
        );
    }
    assert!(
        after.len() * 2 < before.len(),
        "OOV queue should more than halve: {} -> {}",
        before.len(),
        after.len()
    );

    // One extraction row per candidate, no churn.
    assert_eq!(
        scalar_i64(&store, "SELECT COUNT(*) FROM def_extractions").await,
        DEFINITIONS.len() as i64
    );
}

/// The tokenizer refinements have to survive the round trip into `def_tokens`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn possessives_and_abbreviations_never_reach_the_token_table() {
    let fixture = harness();
    let store = fixture.store.clone();
    seed_word(&store, "one", Role::Base).await;
    let word_id = seed_word(&store, "personal", Role::Target).await;
    seed_definition(
        &store,
        word_id,
        "adj",
        "of one's own, e.g. a person's name; vs. that of sb else",
    )
    .await;

    let reconciler = common::local_reconciler(&fixture);
    run_to_quiescence(&reconciler).await;

    let surfaces: Vec<String> = store
        .read(|conn| {
            let mut stmt = conn.prepare("SELECT surface FROM def_tokens ORDER BY position")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
        .unwrap();

    assert_eq!(
        surfaces,
        ["of", "one", "own", "a", "person", "name", "that", "of", "else"]
    );
}

/// A detachment that lands on a non-word must leave the surface alone, so the
/// token stays honestly out of scope instead of inventing a dependency.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lexicon_validation_rejects_a_detachment_that_is_not_a_word() {
    let fixture = harness();
    let store = fixture.store.clone();
    seed_word(&store, "have", Role::Base).await;
    let word_id = seed_word(&store, "gizmo", Role::Target).await;
    seed_definition(&store, word_id, "noun", "a thing having sprockets").await;

    let reconciler = common::local_reconciler(&fixture);
    run_to_quiescence(&reconciler).await;

    let lemmas: Vec<String> = store
        .read(|conn| {
            let mut stmt = conn.prepare("SELECT lemma FROM def_tokens ORDER BY position")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
        .unwrap();

    // `having` resolves because `have` is a word; `sprockets` does not,
    // because `sprocket` is not.
    assert_eq!(lemmas, ["a", "thing", "have", "sprockets"]);
}

/// WNdb's exception files supersede the compiled-in table, and say so in the
/// version they report.
#[test]
fn the_wordnet_exception_files_supersede_the_builtin_table() {
    let dir = tempfile::tempdir().unwrap();
    // `made` disagrees with the built-in table on purpose; `children` is only
    // in the built-in table and must survive the layering.
    std::fs::write(dir.path().join("verb.exc"), b"made mock\nfrobbed frob\n").unwrap();

    let builtin = Arc::new(ExceptionTable::builtin());
    let layered = Arc::new(ExceptionTable::layered(dir.path()));
    assert_eq!(builtin.version(), morpho_reconcile::MORPHY_LEMMATIZER_VER);
    assert_eq!(
        layered.version(),
        morpho_reconcile::MORPHY_LEMMATIZER_WNDB_VER
    );

    let words = ["make", "mock", "child", "frob"];
    let cache = Arc::new(LexiconCache::seeded(words));
    let with_builtin = MorphyLemmatizer::new(builtin, cache.clone());
    let with_wndb = MorphyLemmatizer::new(layered, cache);

    assert_eq!(with_builtin.lemmatize("made"), "make");
    assert_eq!(with_wndb.lemmatize("made"), "mock", "WNdb wins per key");
    assert_eq!(
        with_wndb.lemmatize("children"),
        "child",
        "keys WNdb does not mention keep the fallback"
    );
    assert_eq!(with_wndb.lemmatize("frobbed"), "frob");
    assert_eq!(
        with_builtin.lemmatize("frobbed"),
        "frob",
        "and the plain rules reach it either way"
    );
}

/// The sweep is what keeps the lemmatizer's view of the lexicon current: a word
/// promoted between two passes changes the next extraction's answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_sweep_refreshes_the_lexicon_before_anything_reads_it() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "gizmo", Role::Target).await;
    seed_definition(&store, word_id, "noun", "a thing having parts").await;

    let pipeline = TextPipeline::default();
    assert!(
        pipeline.lexicon().snapshot().is_empty(),
        "nothing is known before the first pass"
    );
    let reconciler = reconciler_with(&fixture, pipeline.clone());
    run_to_quiescence(&reconciler).await;

    let snapshot = pipeline.lexicon().snapshot();
    assert!(snapshot.contains("gizmo"));
    assert!(!snapshot.contains("have"));

    seed_word(&store, "have", Role::Base).await;
    run_to_quiescence(&reconciler).await;
    assert!(pipeline.lexicon().snapshot().contains("have"));
}
