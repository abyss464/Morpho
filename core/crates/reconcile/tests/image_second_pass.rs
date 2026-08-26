//! The second-pass image chain, from the rule that derives it to the row it
//! writes.
//!
//! The derivation half is pinned in `source_priority.rs`. What is left, and
//! what this covers, is the wiring in between: the mark a rule waited on has to
//! be the mark the executor writes, or a stage would derive itself forever and
//! the chain behind it would never open. The only socket here is a loopback one
//! answering canned JSON — nothing reaches the network, and no engine runs.

mod common;

use std::sync::Arc;

use common::{harness, seed_word};
use morpho_domain::job::{JobKey, JobKind, Priority, RateKey, SubjectRef};
use morpho_domain::types::{ImageSource, Role};
use morpho_reconcile::exec::{Executor, FetchImagesExecutor};
use morpho_reconcile::score::ImageStrategy;
use morpho_reconcile::sources::SourceSet;
use morpho_reconcile::{AdapterConfig, EngineContext, JobPayload, JobSpec, SourcesConfig};
use morpho_store::{MediaStore, Store};

/// A loopback server that answers every request with one canned body.
async fn serve(body: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = vec![0u8; 8192];
                let _ = socket.read(&mut buffer).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    base
}

/// One image job, as the second-pass rule builds it.
fn second_pass_job(word_id: i64, lemma: &str, mark: &str, strategy: ImageStrategy) -> JobSpec {
    JobSpec::new(
        JobKey::new(JobKind::FetchImages, SubjectRef::word_source(word_id, mark)),
        RateKey::Openverse,
        Priority::P2,
    )
    .with_payload(JobPayload::FetchImages {
        word_id,
        lemma: lemma.to_string(),
        source: ImageSource::Openverse,
        gloss: Some("a way in which a thing is done".to_string()),
        strategy,
        mark: mark.to_string(),
        gloss_tokens: vec!["way".into(), "thing".into(), "done".into()],
    })
}

async fn marks(store: &Store) -> Vec<(String, i64)> {
    store
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
        .unwrap()
}

/// A pass that finds nothing still finishes, and finishes under its own name.
/// This is the completion marker doing the only job it has (README Part 4
/// §"完成标记") — without it the stage would be derived on every pass forever
/// and the stage behind it would never be derived at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_pass_that_finds_nothing_writes_its_own_mark() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "manner", Role::Target).await;

    let base = serve(r#"{"result_count":0,"results":[]}"#).await;
    let config = SourcesConfig {
        openverse_url: morpho_reconcile::config::OpenverseUrl(format!("{base}/v1/images/")),
        ..SourcesConfig::default()
    };
    let context = Arc::new(EngineContext::new(
        SourceSet::load(config, AdapterConfig::default()).unwrap(),
        MediaStore::new(fixture.dir.path()),
    ));
    let executor = FetchImagesExecutor::new(context);

    executor
        .run(
            &second_pass_job(
                word_id,
                "manner",
                "openverse_relaxed",
                ImageStrategy::RelaxedLicense,
            ),
            &store,
        )
        .await
        .unwrap();

    assert_eq!(marks(&store).await, vec![("openverse_relaxed".into(), 0)]);

    // The next stage marks itself separately; neither touches the strict mark,
    // which is what lets a database that has already been searched walk the
    // whole chain without anything being cleared.
    executor
        .run(
            &second_pass_job(
                word_id,
                "manner",
                "openverse_widened",
                ImageStrategy::WidenedQuery,
            ),
            &store,
        )
        .await
        .unwrap();

    assert_eq!(
        marks(&store).await,
        vec![
            ("openverse_relaxed".to_string(), 0),
            ("openverse_widened".to_string(), 0),
        ]
    );
    assert_eq!(
        store
            .read(|conn| Ok(conn.query_row(
                "SELECT COUNT(*) FROM image_candidates",
                [],
                |row| row.get::<_, i64>(0)
            )?))
            .await
            .unwrap(),
        0
    );
}

/// The strict pass keeps marking itself under the provider's own name, which is
/// the mark every word in the live database already carries.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_strict_pass_still_marks_itself_under_the_provider_name() {
    let fixture = harness();
    let store = fixture.store.clone();
    let word_id = seed_word(&store, "manner", Role::Target).await;

    let base = serve(r#"{"result_count":0,"results":[]}"#).await;
    let config = SourcesConfig {
        openverse_url: morpho_reconcile::config::OpenverseUrl(format!("{base}/v1/images/")),
        ..SourcesConfig::default()
    };
    let context = Arc::new(EngineContext::new(
        SourceSet::load(config, AdapterConfig::default()).unwrap(),
        MediaStore::new(fixture.dir.path()),
    ));

    FetchImagesExecutor::new(context)
        .run(
            &second_pass_job(word_id, "manner", "openverse", ImageStrategy::Strict),
            &store,
        )
        .await
        .unwrap();

    assert_eq!(marks(&store).await, vec![("openverse".into(), 0)]);
}
