//! `morphod serve`: the reconciliation loop and the admin API in one process.

use std::sync::Arc;

use anyhow::{Context, Result};
use morpho_api::{build_router, AppState};
use morpho_reconcile::Reconciler;
use morpho_store::Store;

use crate::config::Config;

pub async fn serve(config: Config, store: Store) -> Result<()> {
    let context = config
        .engine_context()
        .context("resolving content sources")?;
    let reconciler = Reconciler::new(store.clone(), context, config.reconciler_config());
    let registry = reconciler.registry();

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let engine = tokio::spawn(reconciler.run(shutdown_rx));

    let state = AppState::new(
        store,
        Arc::clone(&registry),
        config.data_dir.clone(),
        config.export_settings(),
    )
    .with_releases_dir(config.releases_dir.clone());
    let admin_ui = config.admin_ui_dist.clone();
    let router = build_router(state, Some(admin_ui.as_path()));

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    let local = listener.local_addr().unwrap_or(config.bind);
    tracing::info!(address = %local, "admin API listening");

    let server = axum::serve(listener, router).with_graceful_shutdown(async {
        wait_for_signal().await;
        tracing::info!("shutdown signal received");
    });

    let result = server.await.context("http server failed");

    let _ = shutdown_tx.send(true);
    if let Err(err) = engine.await {
        tracing::warn!(error = %err, "reconciler task ended abnormally");
    }
    result
}

async fn wait_for_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(err) => {
                tracing::warn!(error = %err, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
