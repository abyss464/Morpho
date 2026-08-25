//! `morphod` — the Morpho content engine.
//!
//! One binary, one process: the reconciliation loop, the admin API and the
//! exporter share a single working database and a single transaction view
//! (README Part 2).

mod config;
mod export;
mod import;
mod serve;
mod status;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use tracing_subscriber::EnvFilter;

use morpho_domain::types::Role;
use morpho_store::Store;

use crate::config::Config;

#[derive(Debug, Parser)]
#[command(
    name = "morphod",
    version,
    about = "Morpho content reconciliation engine",
    long_about = None
)]
struct Cli {
    /// Configuration file (defaults to ./morphod.toml when present).
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Override the data directory holding working.db and the media store.
    #[arg(long, global = true, value_name = "DIR")]
    data_dir: Option<PathBuf>,

    /// Log filter, e.g. `info`, `morphod=debug,tower_http=debug`.
    #[arg(long, global = true, value_name = "FILTER", default_value = "info")]
    log: String,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the reconciler and the admin API (default).
    Serve {
        /// Listen address override.
        #[arg(long, value_name = "ADDR")]
        bind: Option<std::net::SocketAddr>,
        /// Built admin UI directory to serve as static files.
        #[arg(long, value_name = "DIR")]
        admin_ui: Option<PathBuf>,
    },
    /// Import a word list (JSONL or one word per line).
    Import {
        /// Path to the word list.
        #[arg(long, value_name = "PATH")]
        wordlist: PathBuf,
        /// Which role the imported words take.
        #[arg(long, value_enum)]
        role: ImportRole,
    },
    /// Print working-database counts and exit.
    Status,
    /// Build a release bundle from the current working state.
    Export {
        /// Bundle directory. Defaults to <releases_dir>/export-<timestamp>.
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
        /// Report the holdback without writing anything.
        #[arg(long)]
        preview: bool,
        /// Recorded in `releases.exported_by`.
        #[arg(long, value_name = "USER", default_value = "cli")]
        actor: String,
        /// Free-text note stored with the release.
        #[arg(long, value_name = "TEXT")]
        notes: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ImportRole {
    Target,
    Base,
}

impl From<ImportRole> for Role {
    fn from(value: ImportRole) -> Self {
        match value {
            ImportRole::Target => Role::Target,
            ImportRole::Base => Role::Base,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.log);

    let mut config = Config::load(cli.config.as_deref())?;
    if let Some(data_dir) = cli.data_dir.clone() {
        config.data_dir = data_dir;
    }

    match cli.command.unwrap_or(Command::Serve {
        bind: None,
        admin_ui: None,
    }) {
        Command::Serve { bind, admin_ui } => {
            if let Some(bind) = bind {
                config.bind = bind;
            }
            if let Some(admin_ui) = admin_ui {
                config.admin_ui_dist = admin_ui;
            }
            let store = open_store(&config)?;
            serve::serve(config, store).await
        }
        Command::Import { wordlist, role } => {
            let store = open_store(&config)?;
            let stats = import::import_wordlist(&store, &wordlist, role.into()).await?;
            println!(
                "imported {} ({}): {} new, {} updated, {} unchanged, {} skipped",
                wordlist.display(),
                Role::from(role),
                stats.inserted,
                stats.updated,
                stats.unchanged,
                stats.skipped
            );
            Ok(())
        }
        Command::Status => {
            let store = open_store(&config)?;
            let report = status::collect(&store, &config.tts).await?;
            print!(
                "{}",
                report.render(
                    &config.working_db().display().to_string(),
                    &config.sources,
                    &config.adapters,
                )
            );
            Ok(())
        }
        Command::Export {
            out,
            preview,
            actor,
            notes,
        } => {
            let store = open_store(&config)?;
            export::run(&config, &store, out, &actor, notes, preview).await
        }
    }
}

fn open_store(config: &Config) -> Result<Store> {
    let store = Store::open(config.store_config())?;
    Ok(store)
}

fn init_tracing(filter: &str) {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(filter))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_subcommand_means_serve() {
        let cli = Cli::try_parse_from(["morphod"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn parses_import() {
        let cli =
            Cli::try_parse_from(["morphod", "import", "--wordlist", "a.txt", "--role", "base"])
                .unwrap();
        match cli.command {
            Some(Command::Import { wordlist, role }) => {
                assert_eq!(wordlist, PathBuf::from("a.txt"));
                assert!(matches!(role, ImportRole::Base));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn rejects_an_unknown_role() {
        assert!(
            Cli::try_parse_from(["morphod", "import", "--wordlist", "a.txt", "--role", "aux"])
                .is_err()
        );
    }
}
