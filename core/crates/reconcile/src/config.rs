//! Engine configuration: which external sources exist at all, and how to reach
//! them.
//!
//! The central idea is **disabled ≡ waived**. A source with no credentials, no
//! data directory or no reachable backend is not "temporarily broken" — it is
//! absent, permanently, until an operator supplies the missing input. Treating
//! it as waived is what lets the fallback chains fire immediately instead of
//! burning eight retries per word first:
//!
//! * no `wordnet_dir` → the WordNet definition fallback and the semantic
//!   grouping stage are simply not part of the desired state;
//! * no `corpus_path` → example fetching is not part of the desired state, and
//!   every word honestly reports `missing_example`;
//! * no image API key → that provider is skipped, and if all three are absent
//!   the SDXL fallback becomes the live path;
//! * no ComfyUI URL → SDXL is absent too, and words honestly report
//!   `missing_image`.
//!
//! Nothing here ever substitutes placeholder content for a missing source.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use morpho_domain::types::ImageSource;

/// Everything the engine needs to know about the outside world.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SourcesConfig {
    /// Free Dictionary API base URL.
    pub freedict_url: FreedictUrl,
    /// Wiktionary MediaWiki API endpoint.
    pub wiktionary_url: WiktionaryUrl,
    /// Directory holding WNdb data files (`data.noun`, `index.noun`, …).
    /// Empty means WordNet is not installed and both WordNet stages are off.
    pub wordnet_dir: Option<PathBuf>,
    /// Exam-corpus JSONL file. Empty means example fetching is off.
    pub corpus_path: Option<PathBuf>,
    pub unsplash_access_key: Option<String>,
    pub pexels_api_key: Option<String>,
    pub pixabay_api_key: Option<String>,
    /// Local ComfyUI base URL for the SDXL fallback.
    pub comfyui_url: Option<String>,
    /// `User-Agent` sent to every HTTP source. Wikimedia requires a real one.
    pub user_agent: UserAgent,
    /// Per-request timeout for HTTP sources, in seconds.
    pub http_timeout_secs: u64,
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            freedict_url: FreedictUrl::default(),
            wiktionary_url: WiktionaryUrl::default(),
            wordnet_dir: None,
            corpus_path: None,
            unsplash_access_key: None,
            pexels_api_key: None,
            pixabay_api_key: None,
            comfyui_url: None,
            user_agent: UserAgent::default(),
            http_timeout_secs: 20,
        }
    }
}

macro_rules! defaulted_string {
    ($name:ident, $default:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl Default for $name {
            fn default() -> Self {
                Self($default.to_string())
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }
    };
}

defaulted_string!(
    FreedictUrl,
    "https://api.dictionaryapi.dev/api/v2/entries/en",
    "Free Dictionary API base; the word is appended as a path segment."
);
defaulted_string!(
    WiktionaryUrl,
    "https://en.wiktionary.org/w/api.php",
    "English Wiktionary MediaWiki API endpoint."
);
defaulted_string!(
    UserAgent,
    "morphod/0.1 (Morpho content engine; +https://github.com/morpho)",
    "User-Agent header sent to every HTTP source."
);

/// One stock-photo provider and its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageProvider {
    pub source: ImageSource,
    pub rate_key: morpho_domain::job::RateKey,
}

/// The three stock-photo providers, in the order README Part 2 lists them.
pub const IMAGE_PROVIDERS: &[ImageProvider] = &[
    ImageProvider {
        source: ImageSource::Unsplash,
        rate_key: morpho_domain::job::RateKey::Unsplash,
    },
    ImageProvider {
        source: ImageSource::Pexels,
        rate_key: morpho_domain::job::RateKey::Pexels,
    },
    ImageProvider {
        source: ImageSource::Pixabay,
        rate_key: morpho_domain::job::RateKey::Pixabay,
    },
];

impl SourcesConfig {
    /// Fill unset fields from the environment.
    ///
    /// Environment beats the file for secrets specifically, because keys do not
    /// belong in a checked-in config.
    pub fn apply_env(&mut self) {
        fn env(name: &str) -> Option<String> {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        }
        if let Some(value) = env("UNSPLASH_ACCESS_KEY") {
            self.unsplash_access_key = Some(value);
        }
        if let Some(value) = env("PEXELS_API_KEY") {
            self.pexels_api_key = Some(value);
        }
        if let Some(value) = env("PIXABAY_API_KEY") {
            self.pixabay_api_key = Some(value);
        }
        if let Some(value) = env("COMFYUI_URL") {
            self.comfyui_url = Some(value);
        }
        if let Some(value) = env("MORPHO_WORDNET_DIR") {
            self.wordnet_dir = Some(PathBuf::from(value));
        }
        if let Some(value) = env("MORPHO_CORPUS_PATH") {
            self.corpus_path = Some(PathBuf::from(value));
        }
    }

    /// API key of one stock-photo provider, if configured.
    pub fn image_key(&self, source: ImageSource) -> Option<&str> {
        let key = match source {
            ImageSource::Unsplash => self.unsplash_access_key.as_deref(),
            ImageSource::Pexels => self.pexels_api_key.as_deref(),
            ImageSource::Pixabay => self.pixabay_api_key.as_deref(),
            ImageSource::Sdxl | ImageSource::Manual => None,
        };
        key.map(str::trim).filter(|value| !value.is_empty())
    }

    /// Stock-photo providers that actually have a key.
    pub fn enabled_image_sources(&self) -> Vec<ImageSource> {
        IMAGE_PROVIDERS
            .iter()
            .map(|p| p.source)
            .filter(|source| self.image_key(*source).is_some())
            .collect()
    }

    /// WordNet's data directory, only if it looks like one.
    ///
    /// A configured-but-empty directory is reported as absent rather than
    /// failing every job: the operator gets one honest "disabled" signal.
    pub fn wordnet_dir(&self) -> Option<&Path> {
        let dir = self.wordnet_dir.as_deref()?;
        if dir.join("data.noun").is_file() {
            Some(dir)
        } else {
            tracing::warn!(
                dir = %dir.display(),
                "wordnet_dir does not contain data.noun; WordNet stages stay disabled"
            );
            None
        }
    }

    /// The exam corpus, only if the file exists.
    pub fn corpus_path(&self) -> Option<&Path> {
        let path = self.corpus_path.as_deref()?;
        if path.is_file() {
            Some(path)
        } else {
            tracing::warn!(
                path = %path.display(),
                "corpus_path does not exist; example fetching stays disabled"
            );
            None
        }
    }

    /// The ComfyUI endpoint, if one is configured.
    pub fn comfyui_url(&self) -> Option<&str> {
        self.comfyui_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn http_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.http_timeout_secs.clamp(1, 300))
    }

    /// One line per source, for the startup log and `morphod status`.
    pub fn describe(&self) -> Vec<(&'static str, String)> {
        let state = |enabled: bool, detail: String| {
            if enabled {
                detail
            } else {
                "disabled".to_string()
            }
        };
        vec![
            ("freedict", self.freedict_url.0.clone()),
            ("wiktionary", self.wiktionary_url.0.clone()),
            (
                "wordnet",
                state(
                    self.wordnet_dir().is_some(),
                    self.wordnet_dir
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                ),
            ),
            (
                "exam_corpus",
                state(
                    self.corpus_path().is_some(),
                    self.corpus_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                ),
            ),
            (
                "unsplash",
                state(
                    self.image_key(ImageSource::Unsplash).is_some(),
                    "key set".into(),
                ),
            ),
            (
                "pexels",
                state(
                    self.image_key(ImageSource::Pexels).is_some(),
                    "key set".into(),
                ),
            ),
            (
                "pixabay",
                state(
                    self.image_key(ImageSource::Pixabay).is_some(),
                    "key set".into(),
                ),
            ),
            (
                "sdxl",
                state(
                    self.comfyui_url().is_some(),
                    self.comfyui_url().unwrap_or_default().to_string(),
                ),
            ),
        ]
    }
}

/// How to invoke the Python adapters (adapter-protocol.md wave-2 ruling #1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdapterConfig {
    /// Directory the adapter commands run from; `adapters/<name>` lives under
    /// it (admin-api.md wave-3 ruling #17).
    ///
    /// `None` means "not resolved yet" and falls back to the process working
    /// directory. morphod fills it in from the config file's location at load
    /// time, which is what makes an adapter spawn independent of where the
    /// daemon happened to be started.
    pub adapters_root: Option<PathBuf>,
    /// Argument vector template. Every `{adapter}` is replaced with the adapter
    /// name, so one template serves all three.
    ///
    /// The default is the invocation the protocol prescribes. It is a template
    /// rather than a fixed string because an operator who has vendored the
    /// virtualenvs wants `python -m morpho_{adapter}` instead, and there is no
    /// reason to make that a code change.
    pub command: Vec<String>,
    /// Batch size for `morfessor.segment` (ruling #4: at least 300 while the
    /// ad-hoc trainer is the only model).
    pub morfessor_batch: usize,
    /// How long a small pending batch may wait before being sent anyway.
    pub morfessor_batch_max_age_secs: u64,
}

/// The invocation from adapter-protocol.md ruling #1.
pub const DEFAULT_ADAPTER_COMMAND: &[&str] = &[
    "uv",
    "run",
    "--project",
    "adapters/{adapter}",
    "{adapter}-adapter",
];

/// Directory under `adapters_root` that holds the per-adapter projects.
pub const ADAPTERS_DIR: &str = "adapters";

/// The three subprocess adapters, and the jobs that dead-letter without each.
///
/// Ruling #17 wants a startup warning that names the damage rather than a vague
/// "adapter unavailable", so the consequence is spelled out next to the name.
pub const ADAPTERS: &[(&str, &str)] = &[
    (
        "tts",
        "synth_tts jobs — no word, sense or example audio is produced",
    ),
    (
        "morfessor",
        "segment_morphology jobs — the etymology fallback after Wiktionary is exhausted",
    ),
    (
        "sdxl",
        "gen_image_sdxl jobs — the image fallback after every stock provider is exhausted",
    ),
];

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            adapters_root: None,
            command: DEFAULT_ADAPTER_COMMAND
                .iter()
                .map(|part| (*part).to_string())
                .collect(),
            morfessor_batch: 300,
            morfessor_batch_max_age_secs: 600,
        }
    }
}

impl AdapterConfig {
    /// Directory every adapter spawn uses as its working directory.
    pub fn root(&self) -> &Path {
        self.adapters_root
            .as_deref()
            .unwrap_or_else(|| Path::new("."))
    }

    /// Command line for one adapter, as `(program, args)`.
    pub fn command(&self, adapter: &str) -> (String, Vec<String>) {
        let mut parts = self
            .command
            .iter()
            .map(|part| part.replace("{adapter}", adapter));
        let program = parts.next().unwrap_or_else(|| "uv".to_string());
        (program, parts.collect())
    }

    /// The launcher binary, before any substitution.
    pub fn runner(&self) -> &str {
        self.command.first().map(String::as_str).unwrap_or("uv")
    }

    /// Absolute project directory of one adapter, when the command template
    /// names one.
    ///
    /// The template owns the layout, so the probe reads it back rather than
    /// assuming `adapters/<name>`: an operator who replaced the invocation with
    /// `python -m morpho_{adapter}` has no project directory to check, and
    /// gets `None` instead of a bogus warning.
    pub fn project_dir(&self, adapter: &str) -> Option<PathBuf> {
        let mut parts = self.command.iter();
        let relative = loop {
            let part = parts.next()?;
            if part == "--project" {
                break parts.next()?.as_str();
            }
            if let Some(value) = part.strip_prefix("--project=") {
                break value;
            }
        };
        let relative = relative.replace("{adapter}", adapter);
        Some(self.root().join(relative))
    }

    pub fn morfessor_batch_max_age(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.morfessor_batch_max_age_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_disable_every_optional_source() {
        let config = SourcesConfig::default();
        assert!(config.wordnet_dir().is_none());
        assert!(config.corpus_path().is_none());
        assert!(config.enabled_image_sources().is_empty());
        assert!(config.comfyui_url().is_none());
        // The two keyless HTTP sources are always available.
        assert!(config.freedict_url.contains("dictionaryapi.dev"));
        assert!(config.wiktionary_url.contains("wiktionary.org"));
    }

    #[test]
    fn a_blank_key_counts_as_absent() {
        let config = SourcesConfig {
            unsplash_access_key: Some("   ".to_string()),
            pexels_api_key: Some(String::new()),
            ..SourcesConfig::default()
        };
        assert!(config.image_key(ImageSource::Unsplash).is_none());
        assert!(config.image_key(ImageSource::Pexels).is_none());
        assert!(config.enabled_image_sources().is_empty());
    }

    #[test]
    fn configured_keys_enable_providers_in_contract_order() {
        let config = SourcesConfig {
            pixabay_api_key: Some("k1".into()),
            unsplash_access_key: Some("k2".into()),
            ..SourcesConfig::default()
        };
        assert_eq!(
            config.enabled_image_sources(),
            vec![ImageSource::Unsplash, ImageSource::Pixabay]
        );
    }

    #[test]
    fn a_wordnet_dir_without_data_files_is_treated_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let config = SourcesConfig {
            wordnet_dir: Some(dir.path().to_path_buf()),
            ..SourcesConfig::default()
        };
        assert!(config.wordnet_dir().is_none());

        std::fs::write(dir.path().join("data.noun"), b"  1 ...").unwrap();
        assert!(config.wordnet_dir().is_some());
    }

    #[test]
    fn a_missing_corpus_file_is_treated_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corpus.jsonl");
        let config = SourcesConfig {
            corpus_path: Some(path.clone()),
            ..SourcesConfig::default()
        };
        assert!(config.corpus_path().is_none());
        std::fs::write(&path, b"").unwrap();
        assert!(config.corpus_path().is_some());
    }

    #[test]
    fn adapter_command_matches_the_protocol_ruling() {
        let config = AdapterConfig::default();
        let (program, args) = config.command("morfessor");
        assert_eq!(program, "uv");
        assert_eq!(
            args,
            vec![
                "run",
                "--project",
                "adapters/morfessor",
                "morfessor-adapter"
            ]
        );
        let (_, args) = config.command("tts");
        assert_eq!(
            args,
            vec!["run", "--project", "adapters/tts", "tts-adapter"]
        );
        assert_eq!(config.runner(), "uv");
    }

    #[test]
    fn the_command_template_is_overridable() {
        let config = AdapterConfig {
            command: vec!["python".into(), "-m".into(), "morpho_{adapter}".into()],
            ..AdapterConfig::default()
        };
        let (program, args) = config.command("sdxl");
        assert_eq!(program, "python");
        assert_eq!(args, vec!["-m", "morpho_sdxl"]);
    }

    #[test]
    fn an_empty_command_falls_back_rather_than_panicking() {
        let config = AdapterConfig {
            command: Vec::new(),
            ..AdapterConfig::default()
        };
        let (program, args) = config.command("tts");
        assert_eq!(program, "uv");
        assert!(args.is_empty());
    }

    #[test]
    fn morfessor_batch_defaults_to_the_ruling_minimum() {
        assert_eq!(AdapterConfig::default().morfessor_batch, 300);
    }

    #[test]
    fn http_timeout_is_clamped_to_something_survivable() {
        let mut config = SourcesConfig::default();
        assert_eq!(config.http_timeout().as_secs(), 20);
        config.http_timeout_secs = 0;
        assert_eq!(config.http_timeout().as_secs(), 1);
        config.http_timeout_secs = 100_000;
        assert_eq!(config.http_timeout().as_secs(), 300);
    }

    #[test]
    fn describe_reports_every_source() {
        let described = SourcesConfig::default().describe();
        let names: Vec<&str> = described.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            vec![
                "freedict",
                "wiktionary",
                "wordnet",
                "exam_corpus",
                "unsplash",
                "pexels",
                "pixabay",
                "sdxl"
            ]
        );
        assert!(described.iter().any(|(_, state)| state == "disabled"));
    }
}
