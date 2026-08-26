//! Configuration.
//!
//! Precedence: command-line flags > environment > config file > defaults.
//! Relative paths resolve against the process working directory, so morphod is
//! normally launched from the repository root where `data/`, `adapters/` and
//! `admin-ui/dist/` live.
//!
//! The adapters are the exception (admin-api.md wave-3 ruling #17): their root
//! is resolved once at load time, from `adapters.adapters_root` or from the
//! config file's own location, so a subprocess spawn does not depend on where
//! the daemon was started.
//!
//! Secrets are the one place environment beats the file: API keys do not belong
//! in a checked-in config, so `UNSPLASH_ACCESS_KEY` and friends override it.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use morpho_domain::tts::TtsConfig;
use morpho_reconcile::{AdapterConfig, PlanParams, SourcesConfig, ADAPTERS_DIR};

/// Config file consulted when `--config` is not given.
pub const DEFAULT_CONFIG_FILE: &str = "morphod.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Directory holding `working.db` and the content-addressed media store.
    pub data_dir: PathBuf,
    /// Where `morphod export` and `POST /releases/export` write bundles.
    pub releases_dir: PathBuf,
    /// Admin API / UI listen address.
    pub bind: SocketAddr,
    /// Built admin UI to serve as static files.
    pub admin_ui_dist: PathBuf,
    pub store: StoreSection,
    pub reconcile: ReconcileSection,
    pub sources: SourcesConfig,
    pub adapters: AdapterConfig,
    pub tts: TtsConfig,
    pub plan: PlanParams,
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
            releases_dir: PathBuf::from("data/releases"),
            bind: "127.0.0.1:8787".parse().expect("valid default address"),
            admin_ui_dist: PathBuf::from("admin-ui/dist"),
            store: StoreSection::default(),
            reconcile: ReconcileSection::default(),
            sources: SourcesConfig::default(),
            adapters: AdapterConfig::default(),
            tts: TtsConfig::default(),
            plan: PlanParams::default(),
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
        config.resolve_adapters_root(path.as_deref());
        config.absolutize_paths()?;
        Ok(config)
    }

    /// Adapter subprocesses run from `adapters_root`, not our cwd, so every
    /// path handed across that boundary (media staging under `data_dir`,
    /// release bundles under `releases_dir`) must be absolute. Anchor relative
    /// values to the launch cwd once, at load time.
    fn absolutize_paths(&mut self) -> Result<()> {
        let cwd = std::env::current_dir().context("resolving working directory")?;
        for dir in [
            &mut self.data_dir,
            &mut self.releases_dir,
            &mut self.admin_ui_dist,
        ] {
            if dir.is_relative() {
                *dir = cwd.join(&*dir);
            }
        }
        Ok(())
    }

    /// Pin the directory adapter subprocesses run from (ruling #17).
    ///
    /// An explicit `adapters_root` wins, resolved against the config file's
    /// directory when it is relative. Otherwise the repository root is found by
    /// walking up from the config file — and then from the working directory —
    /// looking for an `adapters/` directory. When neither search finds one, the
    /// working directory stands, and the startup probe is what reports the
    /// resulting damage.
    fn resolve_adapters_root(&mut self, config_path: Option<&Path>) {
        let config_dir = config_path
            .and_then(Path::parent)
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(Path::to_path_buf);
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        if let Some(configured) = self.adapters.adapters_root.clone() {
            let base = config_dir.unwrap_or_else(|| cwd.clone());
            self.adapters.adapters_root = Some(base.join(configured));
            return;
        }

        let resolved = config_dir
            .as_deref()
            .and_then(repo_root_above)
            .or_else(|| repo_root_above(&cwd))
            .unwrap_or(cwd);
        self.adapters.adapters_root = Some(resolved);
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
        if let Some(value) = std::env::var_os("MORPHOD_RELEASES_DIR") {
            self.releases_dir = PathBuf::from(value);
        }
        if let Some(value) = std::env::var_os("MORPHOD_ADAPTERS_ROOT") {
            self.adapters.adapters_root = Some(PathBuf::from(value));
        }
        self.sources.apply_env();
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

    /// Resolve every external source and build the engine context.
    pub fn engine_context(&self) -> Result<morpho_reconcile::EngineContext> {
        let sources =
            morpho_reconcile::SourceSet::load(self.sources.clone(), self.adapters.clone())?;
        Ok(morpho_reconcile::EngineContext::new(
            sources,
            morpho_store::MediaStore::new(&self.data_dir),
        )
        .with_tts(self.tts.clone())
        .with_plan_params(self.plan)
        .with_pipeline(self.text_pipeline()))
    }

    /// The tokenizer/lemmatizer pair this configuration implies.
    ///
    /// One method, because the engine and the exporter must agree: the
    /// lemmatizer reports a different version when WordNet's exception files
    /// are loaded, and an exporter that assumed otherwise would call every word
    /// stale.
    pub fn text_pipeline(&self) -> morpho_reconcile::TextPipeline {
        morpho_reconcile::TextPipeline::from_wordnet_dir(self.sources.wordnet_dir())
    }

    /// Settings for the exporter. The tokenizer/lemmatizer versions must match
    /// the engine's, or every word would look stale.
    pub fn export_settings(&self) -> morpho_export::ExportSettings {
        let pipeline = self.text_pipeline();
        morpho_export::ExportSettings {
            tts: self.tts.clone(),
            tokenizer_ver: pipeline.tokenizer_ver().to_string(),
            lemmatizer_ver: pipeline.lemmatizer_ver().to_string(),
            data_dir: self.data_dir.clone(),
            exporter: format!("morphod/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// Nearest ancestor of `start` (inclusive) that contains an `adapters/`
/// directory.
fn repo_root_above(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(ADAPTERS_DIR).is_dir())
        .map(Path::to_path_buf)
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
        assert_eq!(config.tts.voice, "en-US-AriaNeural");
        assert_eq!(config.plan.group_max, 20);
        assert_eq!(config.adapters.morfessor_batch, 300);
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
    fn source_and_voice_sections_parse() {
        let toml = r#"
            [sources]
            wordnet_dir = "/usr/share/wordnet"
            corpus_path = "data/exam-corpus.jsonl"
            unsplash_access_key = "abc"

            [tts]
            voice = "en-GB-SoniaNeural"
            word_bitrate_kbps = 64

            [plan]
            group_min = 12
            group_max = 18

            [adapters]
            morfessor_batch = 500
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(
            config.sources.wordnet_dir,
            Some(PathBuf::from("/usr/share/wordnet"))
        );
        assert_eq!(config.sources.unsplash_access_key.as_deref(), Some("abc"));
        assert_eq!(config.tts.voice, "en-GB-SoniaNeural");
        assert_eq!(config.tts.word_bitrate_kbps, 64);
        assert_eq!(config.tts.text_bitrate_kbps, 32, "unset keys keep defaults");
        assert_eq!(config.plan.group_min, 12);
        assert_eq!(config.adapters.morfessor_batch, 500);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = toml::from_str::<Config>("nonsense = 1").unwrap_err();
        assert!(err.to_string().contains("nonsense"));
        let err = toml::from_str::<Config>("[tts]\nnonsense = 1").unwrap_err();
        assert!(err.to_string().contains("nonsense"));
    }

    #[test]
    fn round_trips_through_toml() {
        let config = Config::default();
        let text = toml::to_string(&config).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.working_db(), config.working_db());
        assert_eq!(back.tts, config.tts);
    }

    #[test]
    fn the_example_config_parses() {
        // Guards against the shipped example drifting from the struct.
        let raw = include_str!("../../../morphod.example.toml");
        let config: Config = toml::from_str(raw).expect("example config must parse");
        assert_eq!(config.data_dir, PathBuf::from("data"));
    }

    /// A fake checkout: a directory that owns an `adapters/` tree.
    fn fake_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(ADAPTERS_DIR)).unwrap();
        dir
    }

    /// Ruling #17: the root comes from the config file, not the working
    /// directory, and the search walks up to the checkout.
    #[test]
    fn the_adapters_root_is_found_above_the_config_file() {
        let repo = fake_repo();
        let nested = repo.path().join("deploy").join("etc");
        std::fs::create_dir_all(&nested).unwrap();

        let mut config = Config::default();
        config.resolve_adapters_root(Some(&nested.join("morphod.toml")));
        assert_eq!(config.adapters.root(), repo.path());
    }

    #[test]
    fn loading_a_config_pins_the_adapters_root() {
        let repo = fake_repo();
        let path = repo.path().join("morphod.toml");
        std::fs::write(&path, b"data_dir = \"data\"\n").unwrap();

        let config = Config::load(Some(&path)).unwrap();
        assert_eq!(config.adapters.root(), repo.path());
    }

    #[test]
    fn an_explicit_relative_adapters_root_resolves_against_the_config_file() {
        let repo = fake_repo();
        let mut config = Config::default();
        config.adapters.adapters_root = Some(PathBuf::from("checkout"));
        config.resolve_adapters_root(Some(&repo.path().join("morphod.toml")));
        assert_eq!(config.adapters.root(), repo.path().join("checkout"));
    }

    #[test]
    fn an_absolute_adapters_root_is_taken_verbatim() {
        let repo = fake_repo();
        let elsewhere = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.adapters.adapters_root = Some(elsewhere.path().to_path_buf());
        config.resolve_adapters_root(Some(&repo.path().join("morphod.toml")));
        assert_eq!(config.adapters.root(), elsewhere.path());
    }

    #[test]
    fn a_config_outside_any_checkout_falls_back_to_the_working_directory() {
        let orphan = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.resolve_adapters_root(Some(&orphan.path().join("morphod.toml")));

        let root = config.adapters.root().to_path_buf();
        assert_ne!(root, orphan.path());
        // Whatever it settled on is either a real checkout or the working
        // directory — never a directory that plainly has no adapters.
        assert!(
            root.join(ADAPTERS_DIR).is_dir()
                || Some(&root) == std::env::current_dir().ok().as_ref(),
            "{}",
            root.display()
        );
    }

    #[test]
    fn export_settings_track_the_engine_pipeline() {
        let config = Config::default();
        let settings = config.export_settings();
        let pipeline = config.text_pipeline();
        assert_eq!(settings.tokenizer_ver, pipeline.tokenizer_ver());
        assert_eq!(settings.lemmatizer_ver, pipeline.lemmatizer_ver());
        assert!(settings.exporter.starts_with("morphod/"));
    }

    /// Loading WordNet's exception lists changes how definitions lemmatize, so
    /// it has to change the recorded version too — and the exporter has to
    /// follow, or every word would read as stale.
    #[test]
    fn a_wordnet_dictionary_changes_the_lemmatizer_version_on_both_sides() {
        let plain = Config::default();
        assert_eq!(
            plain.text_pipeline().lemmatizer_ver(),
            morpho_reconcile::MORPHY_LEMMATIZER_VER
        );

        let dir = tempfile::tempdir().unwrap();
        // `wordnet_dir()` gates on data.noun; the lemmatizer wants the .exc files.
        std::fs::write(dir.path().join("data.noun"), b"").unwrap();
        std::fs::write(dir.path().join("verb.exc"), b"gribbled gribble\n").unwrap();
        let mut config = Config::default();
        config.sources.wordnet_dir = Some(dir.path().to_path_buf());

        assert_eq!(
            config.text_pipeline().lemmatizer_ver(),
            morpho_reconcile::MORPHY_LEMMATIZER_WNDB_VER
        );
        assert_eq!(
            config.export_settings().lemmatizer_ver,
            config.text_pipeline().lemmatizer_ver()
        );
        assert_ne!(
            config.export_settings().lemmatizer_ver,
            plain.export_settings().lemmatizer_ver
        );
    }
}
