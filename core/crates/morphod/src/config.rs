//! Configuration.
//!
//! Precedence: command-line flags > environment > config file > defaults.
//! Relative paths resolve against the process working directory, so morphod is
//! normally launched from the repository root where `data/` and
//! `admin-ui/dist/` live.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Config file consulted when `--config` is not given.
pub const DEFAULT_CONFIG_FILE: &str = "morphod.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Directory holding `working.db` and the content-addressed media store.
    pub data_dir: PathBuf,
    /// Admin API / UI listen address.
    pub bind: SocketAddr,
    /// Built admin UI to serve as static files.
    pub admin_ui_dist: PathBuf,
    pub store: StoreSection,
    pub reconcile: ReconcileSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StoreSection {
    /// Read-only connections (README Part 4 says 4–8).
    pub read_pool_size: usize,
    /// Pending write operations before callers start waiting.
    pub write_queue_depth: usize,
    pub change_bus_capacity: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReconcileSection {
    /// Periodic full-pass interval.
    pub full_pass_interval_secs: u64,
    /// Change-event coalescing window.
    pub coalesce_window_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("data"),
            bind: "127.0.0.1:8787".parse().expect("valid default address"),
            admin_ui_dist: PathBuf::from("admin-ui/dist"),
            store: StoreSection::default(),
            reconcile: ReconcileSection::default(),
        }
    }
}

impl Default for StoreSection {
    fn default() -> Self {
        Self {
            read_pool_size: morpho_store::DEFAULT_READ_POOL_SIZE,
            write_queue_depth: 256,
            change_bus_capacity: 1024,
        }
    }
}

impl Default for ReconcileSection {
    fn default() -> Self {
        Self {
            full_pass_interval_secs: 60,
            coalesce_window_ms: 250,
        }
    }
}

impl Config {
    /// Load the config file (if any) and apply environment overrides.
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let path = match explicit {
            Some(path) => Some(path.to_path_buf()),
            None => match std::env::var_os("MORPHOD_CONFIG") {
                Some(value) => Some(PathBuf::from(value)),
                None => {
                    let default = PathBuf::from(DEFAULT_CONFIG_FILE);
                    default.is_file().then_some(default)
                }
            },
        };

        let mut config = match &path {
            Some(path) => {
                let raw = std::fs::read_to_string(path)
                    .with_context(|| format!("reading config {}", path.display()))?;
                toml::from_str::<Config>(&raw)
                    .with_context(|| format!("parsing config {}", path.display()))?
            }
            None => Config::default(),
        };

        config.apply_env()?;
        Ok(config)
    }

    fn apply_env(&mut self) -> Result<()> {
        if let Some(value) = std::env::var_os("MORPHOD_DATA_DIR") {
            self.data_dir = PathBuf::from(value);
        }
        if let Ok(value) = std::env::var("MORPHOD_BIND") {
            self.bind = value
                .parse()
                .with_context(|| format!("MORPHOD_BIND is not a socket address: {value}"))?;
        }
        if let Some(value) = std::env::var_os("MORPHOD_ADMIN_UI_DIST") {
            self.admin_ui_dist = PathBuf::from(value);
        }
        Ok(())
    }

    /// Path of the working database.
    pub fn working_db(&self) -> PathBuf {
        self.data_dir.join("working.db")
    }

    pub fn full_pass_interval(&self) -> Duration {
        Duration::from_secs(self.reconcile.full_pass_interval_secs.max(1))
    }

    pub fn coalesce_window(&self) -> Duration {
        Duration::from_millis(self.reconcile.coalesce_window_ms)
    }

    pub fn store_config(&self) -> morpho_store::StoreConfig {
        morpho_store::StoreConfig {
            path: self.working_db(),
            read_pool_size: self.store.read_pool_size,
            write_queue_depth: self.store.write_queue_depth,
            change_bus_capacity: self.store.change_bus_capacity,
        }
    }

    pub fn reconciler_config(&self) -> morpho_reconcile::ReconcilerConfig {
        morpho_reconcile::ReconcilerConfig {
            full_pass_interval: self.full_pass_interval(),
            coalesce_window: self.coalesce_window(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let config = Config::default();
        assert_eq!(config.working_db(), PathBuf::from("data/working.db"));
        assert_eq!(config.bind.port(), 8787);
        assert_eq!(config.full_pass_interval(), Duration::from_secs(60));
        assert_eq!(config.coalesce_window(), Duration::from_millis(250));
    }

    #[test]
    fn partial_files_merge_onto_defaults() {
        let toml = r#"
            data_dir = "/srv/morpho/data"
            [reconcile]
            coalesce_window_ms = 500
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.data_dir, PathBuf::from("/srv/morpho/data"));
        assert_eq!(config.coalesce_window(), Duration::from_millis(500));
        assert_eq!(config.full_pass_interval(), Duration::from_secs(60));
        assert_eq!(config.bind.port(), 8787);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = toml::from_str::<Config>("nonsense = 1").unwrap_err();
        assert!(err.to_string().contains("nonsense"));
    }

    #[test]
    fn round_trips_through_toml() {
        let config = Config::default();
        let text = toml::to_string(&config).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.working_db(), config.working_db());
    }
}
