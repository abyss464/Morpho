//! Shared harness for the reconciler integration tests.
//!
//! Every source is left unconfigured, and the engines the tests build register
//! only the rules under test. No test in this tree opens a socket or spawns an
//! adapter: the network-facing rules are simply not part of the rule set.

#![allow(dead_code)]

use std::sync::Arc;

use morpho_domain::event::Actor;
use morpho_domain::types::{CreatedBy, DefinitionSource, Role};
use morpho_reconcile::exec::{Executor, ExtractTokensExecutor};
use morpho_reconcile::rules::ExtractTokensRule;
use morpho_reconcile::sources::SourceSet;
use morpho_reconcile::{
    AdapterConfig, EngineContext, PassStats, Reconciler, ReconcilerConfig, Rule, Scope,
    SourcesConfig,
};
use morpho_store::ops::CreateWord;
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

pub async fn scalar_i64(store: &Store, sql: &'static str) -> i64 {
    store
        .read(move |conn| Ok(conn.query_row(sql, [], |row| row.get::<_, i64>(0))?))
        .await
        .unwrap()
}
