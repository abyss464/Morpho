//! External content sources.
//!
//! Two families:
//!
//! * **in-process** — HTTP via reqwest (Free Dictionary, Wiktionary, Wikimedia
//!   Commons, Openverse, Tatoeba, three stock-photo APIs), WordNet parsed from
//!   WNdb files, the exam corpus read from JSONL;
//! * **subprocess** — the Python adapters (`tts`, `morfessor`, `sdxl`) speaking
//!   the envelope in `docs/contracts/adapter-protocol.md`.
//!
//! Every one of them is a pure function from typed input to typed output plus
//! the `Permanent | Transient | RateLimited` taxonomy. None of them sees the
//! database (README Part 4 §"适配器").

pub mod corpus;
pub mod freedict;
pub mod http;
pub mod images;
pub mod proc;
pub mod sentence;
pub mod tatoeba;
pub mod wiktionary;
pub mod wordnet;

use std::sync::Arc;

use crate::config::{AdapterConfig, SourcesConfig};
use corpus::ExamCorpus;
use wordnet::WordNet;

/// Everything the executors reach the outside world through, resolved once at
/// startup.
///
/// Optional sources are `None` when they are not configured. That is the
/// "disabled ≡ waived" rule made concrete: a rule that finds `None` derives no
/// job at all, which lets the fallback chain move on immediately instead of
/// spending eight retries discovering the absence.
#[derive(Clone)]
pub struct SourceSet {
    pub config: Arc<SourcesConfig>,
    pub adapters: Arc<AdapterConfig>,
    pub http: reqwest::Client,
    pub wordnet: Option<Arc<WordNet>>,
    pub corpus: Option<Arc<ExamCorpus>>,
}

impl SourceSet {
    /// Resolve every source. Loading WordNet or the corpus is done once here,
    /// not per job.
    pub fn load(config: SourcesConfig, adapters: AdapterConfig) -> anyhow::Result<Self> {
        let http = http::build_client(&config)?;

        let wordnet = match config.wordnet_dir() {
            Some(dir) => match WordNet::load(dir) {
                Ok(db) => Some(Arc::new(db)),
                Err(err) => {
                    // A broken install is reported and then treated as absent,
                    // exactly like an unconfigured one.
                    tracing::error!(dir = %dir.display(), error = %err, "WordNet failed to load; staying disabled");
                    None
                }
            },
            None => None,
        };

        let corpus = match config.corpus_path() {
            Some(path) => match ExamCorpus::load(path) {
                Ok(corpus) if !corpus.is_empty() => Some(Arc::new(corpus)),
                Ok(_) => {
                    tracing::warn!(path = %path.display(), "exam corpus has no usable rows; staying disabled");
                    None
                }
                Err(err) => {
                    tracing::error!(path = %path.display(), error = %err, "exam corpus failed to load; staying disabled");
                    None
                }
            },
            None => None,
        };

        Ok(Self {
            config: Arc::new(config),
            adapters: Arc::new(adapters),
            http,
            wordnet,
            corpus,
        })
    }

    pub fn has_wordnet(&self) -> bool {
        self.wordnet.is_some()
    }

    pub fn has_corpus(&self) -> bool {
        self.corpus.is_some()
    }

    pub fn has_sdxl(&self) -> bool {
        self.config.comfyui_url().is_some()
    }

    /// Log one line per source so an operator can see at a glance what is live.
    ///
    /// Wave-3 ruling #17: adapters are reported one by one, and a missing one
    /// warns with the jobs it takes down rather than a generic "unavailable".
    pub fn log_availability(&self) {
        for (name, state) in self.config.describe() {
            tracing::info!(source = name, state = %state, "content source");
        }
        tracing::info!(root = %self.adapters.root().display(), "adapters root");
        for probe in proc::probe_adapters(&self.adapters) {
            if probe.available() {
                tracing::info!(adapter = probe.adapter, state = %probe.state(), "adapter");
            } else {
                tracing::warn!(
                    adapter = probe.adapter,
                    state = %probe.state(),
                    dead_letters = probe.dead_letters,
                    "adapter unavailable"
                );
            }
        }
    }
}
