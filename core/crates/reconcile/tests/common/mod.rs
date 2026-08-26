//! Shared harness for the reconciler integration tests.
//!
//! Every source is left unconfigured, and the engines the tests build register
//! only the rules under test. No test in this tree opens a socket or spawns an
//! adapter: the network-facing rules are simply not part of the rule set.

#![allow(dead_code)]

use std::sync::Arc;

use morpho_domain::event::Actor;
use morpho_domain::types::{CreatedBy, DefinitionSource, ExampleSource, MediaKind, Role};
use morpho_reconcile::exec::{Executor, ExtractTokensExecutor};
use morpho_reconcile::rules::ExtractTokensRule;
use morpho_reconcile::sources::{sentence, SourceSet};
use morpho_reconcile::{
    AdapterConfig, EngineContext, Facts, JobSpec, PassStats, Reconciler, ReconcilerConfig, Rule,
    Scope, Snapshot, SourcesConfig,
};
use morpho_store::ops::{CreateWord, MintExampleCandidate, MintImageCandidate};
use morpho_store::{MediaStore, Store, StoreConfig, WriteOp};

pub struct Harness {
    pub dir: tempfile::TempDir,
    pub store: Store,
}

pub fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(StoreConfig::new(dir.path().join("working.db"))).unwrap();
    Harness { dir, store }
}

/// An engine context with every external source absent.
pub fn context(data_dir: &std::path::Path) -> EngineContext {
    let sources =
        SourceSet::load(SourcesConfig::default(), AdapterConfig::default()).expect("source set");
    EngineContext::new(sources, MediaStore::new(data_dir))
}

/// A reconciler that only knows how to extract tokens.
pub fn local_reconciler(harness: &Harness) -> Reconciler {
    let context = Arc::new(context(harness.dir.path()));
    let pipeline = context.pipeline.clone();
    Reconciler::with_parts(
        harness.store.clone(),
        context,
        ReconcilerConfig::default(),
        vec![Arc::new(ExtractTokensRule::new(pipeline.clone())) as Arc<dyn Rule>],
        vec![Arc::new(ExtractTokensExecutor::new(pipeline)) as Arc<dyn Executor>],
    )
}

pub async fn run_to_quiescence(reconciler: &Reconciler) -> PassStats {
    let stats = reconciler.run_once(Scope::Full).await.unwrap();
    reconciler.dispatcher().drain().await;
    stats
}

/// Run passes until nothing changes, so the inline sweep can settle.
pub async fn converge(reconciler: &Reconciler) -> PassStats {
    let mut last = PassStats::default();
    for _ in 0..12 {
        last = run_to_quiescence(reconciler).await;
        if last.dispatched == 0 && last.sweep.is_quiet() {
            break;
        }
    }
    last
}

pub async fn seed_word(store: &Store, lemma: &str, role: Role) -> i64 {
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

pub async fn seed_ranked_word(store: &Store, lemma: &str, role: Role, rank: i64) -> i64 {
    let mut request = CreateWord::new(lemma, role, CreatedBy::Import);
    request.frequency_rank = Some(rank);
    store
        .write(Actor::Cli, WriteOp::CreateWord(request))
        .await
        .unwrap()
        .result
        .word_id()
        .unwrap()
}

pub async fn seed_definition(store: &Store, word_id: i64, pos: &str, text: &str) -> i64 {
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

/// Mint one example candidate, locating the highlight the way every real
/// source does.
pub async fn seed_example(
    store: &Store,
    word_id: i64,
    lemma: &str,
    text: &str,
    source: ExampleSource,
) -> i64 {
    let example = sentence::candidate(text, lemma, Some(format!("{source}:test")))
        .unwrap_or_else(|| panic!("{text:?} does not contain {lemma:?}"));
    store
        .write(
            Actor::Cli,
            WriteOp::MintExampleCandidate(MintExampleCandidate {
                word_id,
                text: example.text,
                hl_start: example.hl_start,
                hl_end: example.hl_end,
                source,
                source_ref: example.source_ref,
                created_by: None,
            }),
        )
        .await
        .unwrap()
        .result
        .cand_id()
        .unwrap()
}

/// Register a byte string in the media library and hang an image candidate off
/// it. The bytes never have to be a real picture — nothing decodes them here.
pub async fn seed_image(
    store: &Store,
    media: &MediaStore,
    word_id: i64,
    bytes: &[u8],
    source: morpho_domain::types::ImageSource,
) -> i64 {
    let stored = media.put_bytes(bytes, MediaKind::Image).unwrap();
    store
        .write(
            Actor::Cli,
            WriteOp::MintImageCandidate(MintImageCandidate {
                word_id,
                pos: None,
                file_hash: stored.file_hash.clone(),
                media: Some(morpho_store::ops::MediaRegistration {
                    file_hash: stored.file_hash,
                    kind: MediaKind::Image,
                    rel_path: stored.rel_path,
                    bytes: stored.bytes,
                }),
                width: Some(1600),
                height: Some(1200),
                source,
                source_ref: Some(format!("{source}:test")),
                license: Some("test licence".into()),
                query_used: None,
                created_by: None,
            }),
        )
        .await
        .unwrap()
        .result
        .cand_id()
        .unwrap()
}

/// Point a word's image slot at one of its candidates, the way a settled sweep
/// would have left it.
pub async fn select_image(store: &Store, word_id: i64, img_cand_id: i64) {
    store
        .write(
            Actor::Reconciler,
            WriteOp::select(
                morpho_domain::types::SlotRef::Image { word_id },
                img_cand_id,
                morpho_domain::types::SelectedBy::Auto,
            ),
        )
        .await
        .unwrap();
}

/// The `file_hash` a word's image slot currently points at.
pub async fn selected_image_hash(store: &Store, word_id: i64) -> Option<String> {
    store
        .read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT c.file_hash FROM image_selections s
                 JOIN image_candidates c ON c.img_cand_id = s.img_cand_id
                 WHERE s.word_id = ?1",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![word_id], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows.into_iter().next())
        })
        .await
        .unwrap()
}

/// Record a fetch completion marker, so a rule sees a source as answered.
pub async fn mark_fetched(store: &Store, kind: &str, word_id: i64, source: &str, count: i64) {
    store
        .write(
            Actor::Cli,
            WriteOp::RecordSourceFetch {
                kind: kind.to_string(),
                word_id,
                source: source.to_string(),
                result_count: count,
            },
        )
        .await
        .unwrap();
}

/// Run one rule against a fresh fact snapshot, the way a pass would.
pub async fn derive(store: &Store, rule: Arc<dyn Rule>) -> Vec<JobSpec> {
    store
        .read(move |conn| {
            let facts = Facts::load(conn)?;
            let scope = Scope::Full;
            rule.derive(&Snapshot {
                conn,
                facts: &facts,
                scope: &scope,
                now: chrono::Utc::now(),
            })
        })
        .await
        .unwrap()
}

/// `(kind, subject_id)` of every derived job, sorted — a stable shape to assert
/// against.
pub fn job_subjects(jobs: &[JobSpec]) -> Vec<String> {
    let mut out: Vec<String> = jobs
        .iter()
        .map(|job| format!("{}/{}", job.key.kind, job.key.subject.subject_id))
        .collect();
    out.sort();
    out
}

pub async fn scalar_i64(store: &Store, sql: &'static str) -> i64 {
    store
        .read(move |conn| Ok(conn.query_row(sql, [], |row| row.get::<_, i64>(0))?))
        .await
        .unwrap()
}
