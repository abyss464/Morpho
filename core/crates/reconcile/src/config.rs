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
//! * no `corpus_path` → the exam-corpus example source is absent, and only the
//!   keyless ones remain;
//! * no image API key → that stock provider is skipped;
//! * no ComfyUI URL → SDXL is absent too, and words honestly report
//!   `missing_image`.
//!
//! The keyless sources are the other half of the rule (admin-api.md ruling
//! #18). Free Dictionary, Wiktionary, Wikimedia Commons, Openverse and Tatoeba
//! have no credentials to be missing, so they are never disabled — only
//! reachable or not, which is a retry, not an absence. That is what makes an
//! installation with no accounts at all still produce real definitions,
//! sentences and pictures.
//!
//! Nothing here ever substitutes placeholder content for a missing source.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use morpho_domain::types::ImageSource;

use crate::score::ImageStrategy;

/// Everything the engine needs to know about the outside world.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SourcesConfig {
    /// Free Dictionary API base URL.
    pub freedict_url: FreedictUrl,
    /// Wiktionary MediaWiki API endpoint.
    pub wiktionary_url: WiktionaryUrl,
    /// Wikimedia Commons MediaWiki API endpoint (keyless image source).
    pub wikimedia_url: WikimediaUrl,
    /// English Wikipedia REST summary base, used for the article-lead image
    /// strategy. A different endpoint from `wikimedia_url`: the summary lives
    /// on the language wiki, the licence lives on Commons.
    pub wikipedia_url: WikipediaUrl,
    /// Openverse image search endpoint (keyless image source).
    pub openverse_url: OpenverseUrl,
    /// Tatoeba sentence search endpoint (keyless example source).
    pub tatoeba_url: TatoebaUrl,
    /// Directory holding WNdb data files (`data.noun`, `index.noun`, …).
    /// Empty means WordNet is not installed and both WordNet stages are off.
    pub wordnet_dir: Option<PathBuf>,
    /// Exam-corpus JSONL file. Empty means that one example source is off; the
    /// keyless ones carry on regardless.
    pub corpus_path: Option<PathBuf>,
    pub unsplash_access_key: Option<String>,
    pub pexels_api_key: Option<String>,
    pub pixabay_api_key: Option<String>,
    /// Local ComfyUI base URL for the SDXL fallback.
    pub comfyui_url: Option<String>,
    /// Name of (or path to) the codex generator binary the adapter shells out
    /// to. The adapter reads the same `MORPHO_CODEX_BIN` override, so both
    /// sides agree about whether the generator exists — which is what lets an
    /// absent one *disable* the source instead of dead-lettering every word.
    pub codex_bin: Option<String>,
    /// Base URL of the CLIP scoring sidecar (`adapters/clip`, run host-side).
    ///
    /// Empty means image selection has no semantic term at all and ranks on the
    /// quality prior alone — the behaviour that predates it. This is the one
    /// switch that turns the whole feature on, and it is unset by default
    /// because the sidecar needs a GPU the engine's container does not have.
    pub clip_url: Option<String>,
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
            wikimedia_url: WikimediaUrl::default(),
            wikipedia_url: WikipediaUrl::default(),
            openverse_url: OpenverseUrl::default(),
            tatoeba_url: TatoebaUrl::default(),
            wordnet_dir: None,
            corpus_path: None,
            unsplash_access_key: None,
            pexels_api_key: None,
            pixabay_api_key: None,
            comfyui_url: None,
            codex_bin: None,
            clip_url: None,
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
    WikimediaUrl,
    "https://commons.wikimedia.org/w/api.php",
    "Wikimedia Commons MediaWiki API endpoint."
);
defaulted_string!(
    WikipediaUrl,
    "https://en.wikipedia.org/api/rest_v1/page/summary",
    "English Wikipedia REST summary base; the article title is appended as a \
     path segment.\n\n\
     This is the second image strategy: abstract and common words own no file \
     in the Commons `File:` namespace but do own an article whose lead image \
     depicts them. The picture itself still comes from Commons, which is where \
     its licence is."
);
defaulted_string!(
    OpenverseUrl,
    "https://api.openverse.org/v1/images/",
    "Openverse image search endpoint."
);
defaulted_string!(
    ScenePromptVer,
    "scene/1",
    "Version tag of the scene-prompt template.\n\n\
     It rides in the candidate's `source_ref` and in the job subject, so bumping \
     it is how an operator asks for every scene image to be generated again \
     under a changed template. Nothing is cleared: the old candidates stay, \
     carrying the version that produced them."
);
defaulted_string!(
    SdxlWorkflow,
    "sdxl_turbo_v1",
    "ComfyUI workflow template the sdxl adapter should render with.\n\n\
     Passed through as an optional parameter; the adapter owns the file and is \
     free to ignore a name it does not know."
);
defaulted_string!(
    ClipModel,
    "ViT-B-32/laion2b_s34b_b79k",
    "CLIP model identity the sidecar is expected to be serving.\n\n\
     It rides in `clip_scores.model_ver` next to the algorithm version, so a \
     different model writes different rows rather than making the old ones \
     wrong. The executor refuses a sidecar that reports something else — a \
     silent model swap would leave one lexicon scored two ways, which is \
     exactly the failure the version column exists to make impossible."
);
defaulted_string!(
    CodexPromptVer,
    "codex/1",
    "Version tag of the codex generation prompt.\n\n\
     Same mechanism as the scene-prompt version: it rides in the candidate's \
     `source_ref` and in the job subject, so bumping it asks for every word to \
     be generated again under the changed prompt without clearing anything."
);
defaulted_string!(
    TatoebaUrl,
    "https://tatoeba.org/en/api_v0/search",
    "Tatoeba sentence search endpoint."
);
defaulted_string!(
    UserAgent,
    "Morpho/0.1 vocabulary content builder (+https://github.com/morpho)",
    "User-Agent header sent to every HTTP source.\n\n\
     Wikimedia's policy requires a real one — a tool name and a way to make \
     contact — and answers 403 without it, so this is a functional requirement \
     rather than politeness."
);

/// How the image chain behaves, as opposed to where it fetches from.
///
/// Everything here is about the **last** link in that chain. The libraries are
/// asked first and asked again on looser terms, and only a word none of them
/// could answer for reaches generation — that ordering is unchanged, and a real
/// photograph still outranks anything generated. What scene mode changes is the
/// quality of the fallback once a word gets there: instead of prompting for the
/// bare concept, the prompt describes the scene of the word's own slot-1
/// example sentence, which is the picture the card actually wants beside it.
///
/// Off by default. Turning it on is an operator decision, because it points a
/// local GPU at every word the libraries left behind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImagesConfig {
    /// Generate scene images from the slot-1 sentence rather than the bare
    /// concept. `MORPHO_SCENE_MODE` overrides it.
    pub scene_mode: bool,
    /// Version of the scene-prompt template; see [`ScenePromptVer`].
    pub scene_prompt_ver: ScenePromptVer,
    /// Sampler steps sent with a generation request while scene mode is on.
    /// The default suits SDXL-Turbo, which is what makes a bulk pass over
    /// thousands of words finish in an evening.
    pub sdxl_steps: u32,
    /// Classifier-free guidance scale. Turbo wants 1.0 — it was distilled
    /// without guidance, and anything higher scorches the image.
    pub sdxl_cfg: f32,
    /// Workflow template name the adapter should render with.
    pub sdxl_workflow: SdxlWorkflow,
    /// CLIP model the sidecar is expected to serve; see [`ClipModel`].
    pub clip_model: ClipModel,
    /// Offer words the CLIP sidecar rates poorly to the codex generator.
    ///
    /// Off by default, and doubly gated: the source also needs its adapter on
    /// disk. Turning it on points an external generation service at every word
    /// the libraries and SDXL between them could not picture aptly, which costs
    /// somebody's quota — so it is an operator decision, exactly like scene
    /// mode.
    pub codex_enabled: bool,
    /// Version of the codex prompt template; see [`CodexPromptVer`].
    pub codex_prompt_ver: CodexPromptVer,
    /// Raw CLIP cosine below which a word's best picture is judged inapt enough
    /// to be worth generating a replacement for.
    ///
    /// The default is the middle of the band the live lexicon actually occupies:
    /// `ops/clip_rematch.py`'s pass over 4 000 words put a good match around
    /// 0.28 and left the bottom decile under 0.20. A word above it has a picture
    /// somebody can answer a quiz on; a word below it usually has a picture of
    /// the wrong thing.
    pub codex_threshold: f64,
    /// Most codex jobs one derivation may ask for.
    ///
    /// The queue is derived, not stored, so an underived job costs nothing and
    /// comes back next pass — this is what keeps a first run from putting six
    /// thousand generation requests in front of somebody's rate limit.
    pub codex_batch: usize,
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            scene_mode: false,
            scene_prompt_ver: ScenePromptVer::default(),
            sdxl_steps: 4,
            sdxl_cfg: 1.0,
            sdxl_workflow: SdxlWorkflow::default(),
            clip_model: ClipModel::default(),
            codex_enabled: false,
            codex_prompt_ver: CodexPromptVer::default(),
            codex_threshold: 0.22,
            codex_batch: 8,
        }
    }
}

impl ImagesConfig {
    /// Fill unset fields from the environment.
    pub fn apply_env(&mut self) {
        self.apply_flag(SCENE_MODE_ENV, |images, value| images.scene_mode = value);
        self.apply_flag(CODEX_ENABLED_ENV, |images, value| {
            images.codex_enabled = value
        });
        if let Some(value) = env_text(CLIP_MODEL_ENV) {
            self.clip_model = ClipModel(value);
        }
        if let Some(value) = env_text(CODEX_PROMPT_VER_ENV) {
            self.codex_prompt_ver = CodexPromptVer(value);
        }
        if let Some(raw) = env_text(CODEX_THRESHOLD_ENV) {
            match raw.parse::<f64>() {
                Ok(value) if value.is_finite() => self.codex_threshold = value,
                _ => tracing::warn!(
                    value = raw,
                    "{CODEX_THRESHOLD_ENV} is not a number; leaving the threshold as configured"
                ),
            }
        }
        if let Some(raw) = env_text(CODEX_BATCH_ENV) {
            match raw.parse::<usize>() {
                Ok(value) => self.codex_batch = value,
                Err(_) => tracing::warn!(
                    value = raw,
                    "{CODEX_BATCH_ENV} is not a whole number; leaving the batch as configured"
                ),
            }
        }
    }

    /// Read one boolean switch, refusing to guess at anything that is not one.
    fn apply_flag(&mut self, name: &str, set: impl Fn(&mut Self, bool)) {
        let Ok(raw) = std::env::var(name) else {
            return;
        };
        match parse_flag(&raw) {
            Some(value) => set(self, value),
            None if raw.trim().is_empty() => {}
            None => tracing::warn!(
                value = raw,
                "{name} is not a boolean; leaving it as configured"
            ),
        }
    }

    /// The identity CLIP scores are stored under: algorithm version and model,
    /// together, because either one changing means a different number.
    pub fn clip_model_ver(&self) -> String {
        format!(
            "{}:{}",
            morpho_domain::version::CLIP_ALGO_VER,
            self.clip_model.0
        )
    }

    /// The current codex prompt version, as it appears in a `source_ref`.
    pub fn codex_prompt_ver(&self) -> &str {
        &self.codex_prompt_ver
    }

    /// `source_fetch.source` / job subject suffix for the codex pass.
    pub fn codex_mark(&self) -> String {
        format!(
            "codex_{}",
            self.codex_prompt_ver()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        )
    }

    /// The current scene-prompt version, as it appears in a `source_ref`.
    pub fn scene_prompt_ver(&self) -> &str {
        &self.scene_prompt_ver
    }

    /// `source_fetch.source` / job subject suffix for the scene pass.
    ///
    /// The version is part of it, which is the whole mechanism behind "bump the
    /// template and every word derives again": a bumped version is a subject
    /// that has never been dispatched, so no job state, no completion mark and
    /// no dead letter from the previous template applies to it.
    pub fn scene_mark(&self) -> String {
        format!(
            "sdxl_{}",
            self.scene_prompt_ver()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        )
    }
}

/// Environment override for [`ImagesConfig::scene_mode`].
pub const SCENE_MODE_ENV: &str = "MORPHO_SCENE_MODE";
/// Environment override for [`ImagesConfig::clip_model`].
pub const CLIP_MODEL_ENV: &str = "MORPHO_CLIP_MODEL";
/// Environment override for [`ImagesConfig::codex_enabled`].
pub const CODEX_ENABLED_ENV: &str = "MORPHO_CODEX_ENABLED";
/// Environment override for [`ImagesConfig::codex_prompt_ver`].
pub const CODEX_PROMPT_VER_ENV: &str = "MORPHO_CODEX_PROMPT_VER";
/// Environment override for [`ImagesConfig::codex_threshold`].
pub const CODEX_THRESHOLD_ENV: &str = "MORPHO_CODEX_THRESHOLD";
/// Environment override for [`ImagesConfig::codex_batch`].
pub const CODEX_BATCH_ENV: &str = "MORPHO_CODEX_BATCH";

/// A non-empty environment value, trimmed.
fn env_text(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// A boolean spelled the way a shell profile spells one.
///
/// Both directions are recognised, so an operator who exported the variable
/// once can turn scene mode back off without editing the profile — and anything
/// else is neither, which the caller reports rather than silently reading as
/// false.
fn parse_flag(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// One image provider and how to reach it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageProvider {
    pub source: ImageSource,
    pub rate_key: morpho_domain::job::RateKey,
    /// `true` for the stock libraries, which are absent without a key; `false`
    /// for the open collections, which are always available (ruling #18).
    pub needs_key: bool,
    /// Minimum gap between this provider's *photo downloads*, in milliseconds.
    ///
    /// The dispatcher's lane meters jobs, and one image job is a search plus up
    /// to four downloads — five requests, often to a different host than the
    /// search. Left unpaced, a lane running at its seeded 60 jobs a minute puts
    /// three hundred requests a minute on the file host, which is how a live
    /// run against Wikimedia spends its time collecting 429s instead of
    /// pictures. Spacing them inside the job is what turns the lane's job
    /// budget into a request budget the file host will actually serve.
    pub download_spacing_ms: u64,
}

impl ImageProvider {
    pub const fn download_spacing(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.download_spacing_ms)
    }

    /// The provider record for one source, if it is one that gets searched.
    pub fn for_source(source: ImageSource) -> Option<&'static Self> {
        IMAGE_PROVIDERS
            .iter()
            .find(|provider| provider.source == source)
    }
}

/// Every image provider, in priority order (admin-api.md ruling #18): keyed
/// stock libraries first when they are configured, then the open collections,
/// with SDXL behind all of them as the generative fallback.
pub const IMAGE_PROVIDERS: &[ImageProvider] = &[
    ImageProvider {
        source: ImageSource::Unsplash,
        rate_key: morpho_domain::job::RateKey::Unsplash,
        needs_key: true,
        // A paid CDN serving exactly this traffic, and the key already meters
        // the account.
        download_spacing_ms: 0,
    },
    ImageProvider {
        source: ImageSource::Pexels,
        rate_key: morpho_domain::job::RateKey::Pexels,
        needs_key: true,
        download_spacing_ms: 0,
    },
    ImageProvider {
        source: ImageSource::Pixabay,
        rate_key: morpho_domain::job::RateKey::Pixabay,
        needs_key: true,
        download_spacing_ms: 0,
    },
    ImageProvider {
        source: ImageSource::Wikimedia,
        rate_key: morpho_domain::job::RateKey::Wikimedia,
        needs_key: false,
        // `upload.wikimedia.org` is a donated file host with no account behind
        // the request, and it answers 429 long before the API does — measured
        // against a live run, the API served every search while the thumbnails
        // throttled. It will hold about a request a second in total, and the
        // lane runs `max_concurrency` jobs at once, so the gap inside one job
        // is that budget divided among them.
        //
        // The 429 handling is still the backstop and still correct; this only
        // stops the lane spending its time parked. Going from no gap to 1.2 s
        // cut the parks from three a second to one a minute, and the second
        // second removes them.
        download_spacing_ms: 2_000,
    },
    ImageProvider {
        source: ImageSource::Openverse,
        rate_key: morpho_domain::job::RateKey::Openverse,
        needs_key: false,
        // The files come from whichever third party Openverse indexed, so the
        // load is spread; a smaller gap is enough to stay polite.
        download_spacing_ms: 400,
    },
];

/// One second-pass image search: the same provider, asked again on looser
/// terms after the first pass left a word with nothing.
///
/// Each pass owns a **completion mark of its own** (`source_fetch.source`),
/// which is the whole reason the second passes can be added to a database that
/// has already been searched: the strict marks stay exactly where they are, and
/// every word re-enters through a mark that has never been written. The
/// candidate a pass produces still records the provider it came from —
/// `image_candidates.source` has a `CHECK` union that these names are not part
/// of, and lying to it would be a schema error as well as a false provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSecondPass {
    /// Provider queried, and the candidate's `source`.
    pub source: ImageSource,
    /// `source_fetch.source` this pass writes.
    pub mark: &'static str,
    pub strategy: ImageStrategy,
    pub rate_key: morpho_domain::job::RateKey,
}

/// The second passes, in the order a word with no picture walks them.
///
/// Relaxed licence before widened query, because a hit that still names the
/// word is more likely to depict it than a hit on the gloss's vocabulary — and
/// Wikimedia before Openverse for the same reason ruling #18 orders them that
/// way, Commons being the more deliberately catalogued of the two.
///
/// Every entry is keyless. A second pass over a stock library would be a second
/// request against a metered account for a word the library has already said it
/// has nothing for.
pub const IMAGE_SECOND_PASSES: &[ImageSecondPass] = &[
    ImageSecondPass {
        source: ImageSource::Openverse,
        mark: "openverse_relaxed",
        strategy: ImageStrategy::RelaxedLicense,
        rate_key: morpho_domain::job::RateKey::Openverse,
    },
    ImageSecondPass {
        source: ImageSource::Wikimedia,
        mark: "wikimedia_widened",
        strategy: ImageStrategy::WidenedQuery,
        rate_key: morpho_domain::job::RateKey::Wikimedia,
    },
    ImageSecondPass {
        source: ImageSource::Openverse,
        mark: "openverse_widened",
        strategy: ImageStrategy::WidenedQuery,
        rate_key: morpho_domain::job::RateKey::Openverse,
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
        if let Some(value) = env(CLIP_URL_ENV) {
            self.clip_url = Some(value);
        }
        if let Some(value) = env(CODEX_BIN_ENV) {
            self.codex_bin = Some(value);
        }
        if let Some(value) = env("MORPHO_WORDNET_DIR") {
            self.wordnet_dir = Some(PathBuf::from(value));
        }
        if let Some(value) = env("MORPHO_CORPUS_PATH") {
            self.corpus_path = Some(PathBuf::from(value));
        }
    }

    /// API key of one image provider, if it takes one and it is configured.
    pub fn image_key(&self, source: ImageSource) -> Option<&str> {
        let key = match source {
            ImageSource::Unsplash => self.unsplash_access_key.as_deref(),
            ImageSource::Pexels => self.pexels_api_key.as_deref(),
            ImageSource::Pixabay => self.pixabay_api_key.as_deref(),
            // The keyless providers, and the two that are not searched at all.
            ImageSource::Wikimedia
            | ImageSource::Openverse
            | ImageSource::Sdxl
            | ImageSource::Codex
            | ImageSource::Manual => None,
        };
        key.map(str::trim).filter(|value| !value.is_empty())
    }

    /// Image providers that will actually answer, in priority order.
    ///
    /// A keyed library is here only when its key is set; the keyless ones are
    /// always here, which is what ruling #18 means by "wikimedia and openverse
    /// always enabled".
    pub fn enabled_image_sources(&self) -> Vec<ImageSource> {
        IMAGE_PROVIDERS
            .iter()
            .filter(|provider| !provider.needs_key || self.image_key(provider.source).is_some())
            .map(|provider| provider.source)
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

    /// The codex generator binary to look for.
    pub fn codex_bin(&self) -> &str {
        self.codex_bin
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_CODEX_BIN)
    }

    /// The CLIP sidecar endpoint, if one is configured.
    ///
    /// Absent is the ordinary case rather than a fault: the sidecar wants a GPU,
    /// and the engine's own container has none. Image selection then ranks on
    /// the quality prior alone, which is exactly what it did before semantic
    /// scoring existed — "disabled ≡ waived", applied to a scorer.
    pub fn clip_url(&self) -> Option<&str> {
        self.clip_url
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
            ("wikimedia", self.wikimedia_url.0.clone()),
            ("wikipedia", self.wikipedia_url.0.clone()),
            ("openverse", self.openverse_url.0.clone()),
            ("tatoeba", self.tatoeba_url.0.clone()),
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
            (
                "clip",
                state(
                    self.clip_url().is_some(),
                    self.clip_url().unwrap_or_default().to_string(),
                ),
            ),
            ("codex", format!("`{}`", self.codex_bin())),
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

/// The subprocess adapters, and the jobs that dead-letter without each.
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
    (
        "codex",
        "gen_image_codex jobs — the last image source, for words nothing pictures aptly",
    ),
    (
        "clip",
        "score_image_clip jobs — no semantic scoring, image selection ranks on quality alone",
    ),
];

/// Environment override for [`SourcesConfig::clip_url`].
pub const CLIP_URL_ENV: &str = "MORPHO_CLIP_URL";
/// Environment override for [`SourcesConfig::codex_bin`]. The codex adapter
/// reads the same variable.
pub const CODEX_BIN_ENV: &str = "MORPHO_CODEX_BIN";
/// The generator binary's name when nobody says otherwise.
pub const DEFAULT_CODEX_BIN: &str = "codex";

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

    /// Every provider that takes a key.
    const KEYED: &[ImageSource] = &[
        ImageSource::Unsplash,
        ImageSource::Pexels,
        ImageSource::Pixabay,
    ];
    /// Every provider that does not (ruling #18).
    const KEYLESS: &[ImageSource] = &[ImageSource::Wikimedia, ImageSource::Openverse];

    #[test]
    fn defaults_disable_every_credentialed_source() {
        let config = SourcesConfig::default();
        assert!(config.wordnet_dir().is_none());
        assert!(config.corpus_path().is_none());
        assert!(config.comfyui_url().is_none());
        assert!(config.clip_url().is_none());
        for source in KEYED {
            assert!(config.image_key(*source).is_none(), "{source}");
        }
    }

    /// Ruling #18: with no credentials anywhere, the keyless sources are still
    /// live — that is the difference between "no accounts" and "no content".
    #[test]
    fn the_keyless_sources_survive_an_empty_configuration() {
        let config = SourcesConfig::default();
        assert_eq!(config.enabled_image_sources(), KEYLESS.to_vec());
        assert!(config.freedict_url.contains("dictionaryapi.dev"));
        assert!(config.wiktionary_url.contains("wiktionary.org"));
        assert!(config.wikimedia_url.contains("commons.wikimedia.org"));
        assert!(config.wikipedia_url.contains("en.wikipedia.org"));
        assert!(config.openverse_url.contains("api.openverse.org"));
        assert!(config.tatoeba_url.contains("tatoeba.org"));
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
        assert_eq!(config.enabled_image_sources(), KEYLESS.to_vec());
    }

    /// Ruling #18's priority order: keyed stock libraries ahead of the open
    /// collections, and a provider only appears once its key exists.
    #[test]
    fn configured_keys_enable_providers_in_priority_order() {
        let config = SourcesConfig {
            pixabay_api_key: Some("k1".into()),
            unsplash_access_key: Some("k2".into()),
            ..SourcesConfig::default()
        };
        assert_eq!(
            config.enabled_image_sources(),
            vec![
                ImageSource::Unsplash,
                ImageSource::Pixabay,
                ImageSource::Wikimedia,
                ImageSource::Openverse,
            ]
        );
    }

    /// A lane meters jobs; a job makes several requests. The open collections
    /// have no account behind them and answer 429 first, so they are the ones
    /// that need the gap.
    #[test]
    fn the_keyless_providers_pace_their_downloads() {
        for source in KEYLESS {
            let provider = ImageProvider::for_source(*source).unwrap();
            assert!(
                !provider.download_spacing().is_zero(),
                "{source} would hammer its file host"
            );
        }
        for source in KEYED {
            assert!(
                ImageProvider::for_source(*source)
                    .unwrap()
                    .download_spacing()
                    .is_zero(),
                "{source} is metered by its own key"
            );
        }
        assert!(ImageProvider::for_source(ImageSource::Sdxl).is_none());
        assert!(ImageProvider::for_source(ImageSource::Manual).is_none());
    }

    #[test]
    fn the_provider_table_agrees_with_the_key_lookup() {
        let all = SourcesConfig {
            unsplash_access_key: Some("k".into()),
            pexels_api_key: Some("k".into()),
            pixabay_api_key: Some("k".into()),
            ..SourcesConfig::default()
        };
        for provider in IMAGE_PROVIDERS {
            assert_eq!(
                provider.needs_key,
                all.image_key(provider.source).is_some(),
                "{} disagrees with its key lookup",
                provider.source
            );
        }
        assert_eq!(
            IMAGE_PROVIDERS
                .iter()
                .filter(|p| !p.needs_key)
                .map(|p| p.source)
                .collect::<Vec<_>>(),
            KEYLESS.to_vec()
        );
    }

    /// A second pass writes its own completion mark, and that mark must not
    /// collide with any strict one — otherwise it would overwrite the record of
    /// the first pass instead of adding to it.
    #[test]
    fn every_second_pass_mark_is_its_own() {
        let mut marks: Vec<&str> = IMAGE_SECOND_PASSES.iter().map(|pass| pass.mark).collect();
        let count = marks.len();
        marks.sort_unstable();
        marks.dedup();
        assert_eq!(marks.len(), count, "two passes share a mark");
        for source in ImageSource::ALL {
            assert!(
                !marks.contains(&source.as_str()),
                "{source} would have its strict mark overwritten"
            );
        }
    }

    /// Only the open collections get a second pass: a keyed library is metered
    /// by an account, and asking it twice for a word it has nothing for spends
    /// that account's quota on a certain miss.
    #[test]
    fn the_second_passes_are_keyless_and_lane_matched() {
        let config = SourcesConfig::default();
        for pass in IMAGE_SECOND_PASSES {
            assert!(
                KEYLESS.contains(&pass.source),
                "{} takes a key",
                pass.source
            );
            assert!(config.enabled_image_sources().contains(&pass.source));
            // The pass rides its provider's own lane, so a second pass cannot
            // outrun the rate limit the first pass respects.
            let provider = ImageProvider::for_source(pass.source).unwrap();
            assert_eq!(pass.rate_key, provider.rate_key);
            assert_ne!(pass.strategy, ImageStrategy::Strict);
        }
    }

    // -- scene mode ---------------------------------------------------------

    /// Scene mode costs GPU time on every word the libraries left behind, so it
    /// is never something a checkout falls into.
    #[test]
    fn scene_mode_is_off_until_an_operator_says_otherwise() {
        let images = ImagesConfig::default();
        assert!(!images.scene_mode);
        assert_eq!(images.scene_prompt_ver(), "scene/1");
    }

    /// Turbo's settings, which is what makes a bulk pass finishable.
    #[test]
    fn the_generation_defaults_suit_the_fast_checkpoint() {
        let images = ImagesConfig::default();
        assert_eq!(images.sdxl_steps, 4);
        assert!((images.sdxl_cfg - 1.0).abs() < f32::EPSILON);
        assert_eq!(&*images.sdxl_workflow, "sdxl_turbo_v1");
    }

    /// The mark carries the version, which is the entire mechanism behind
    /// "bump the template and every word derives again".
    #[test]
    fn the_scene_mark_carries_the_template_version() {
        let mut images = ImagesConfig::default();
        assert_eq!(images.scene_mark(), "sdxl_scene_1");
        images.scene_prompt_ver = ScenePromptVer("scene/2".into());
        assert_eq!(images.scene_mark(), "sdxl_scene_2");
        // And a mark is never confusable with a library's own.
        for source in ImageSource::ALL {
            assert_ne!(images.scene_mark(), source.as_str());
        }
        for pass in IMAGE_SECOND_PASSES {
            assert_ne!(images.scene_mark(), pass.mark);
        }
    }

    // -- semantic scoring and the codex source ------------------------------

    /// Both wave-9 switches cost somebody else's hardware, so neither is
    /// something a checkout falls into.
    #[test]
    fn semantic_scoring_and_codex_generation_are_both_opt_in() {
        let config = SourcesConfig::default();
        let images = ImagesConfig::default();
        assert!(config.clip_url().is_none(), "no sidecar until one is named");
        assert_eq!(config.codex_bin(), DEFAULT_CODEX_BIN);
        assert!(!images.codex_enabled);
        // …but the identity a score would be stored under is settled anyway, so
        // turning the sidecar on never leaves rows keyed on an empty model.
        assert_eq!(
            images.clip_model_ver(),
            "clip/1:ViT-B-32/laion2b_s34b_b79k",
            "the stored identity must name both the algorithm and the model"
        );
    }

    /// A blank URL is an operator who exported the variable and then cleared it,
    /// which is a request to turn the sidecar off, not a request to call "".
    #[test]
    fn a_blank_clip_url_counts_as_absent() {
        let config = SourcesConfig {
            clip_url: Some("   ".into()),
            ..SourcesConfig::default()
        };
        assert!(config.clip_url().is_none());
    }

    /// The codex trigger sits inside the band the live lexicon occupies: high
    /// enough to catch a picture of the wrong thing, low enough that a merely
    /// mediocre photograph is left alone.
    #[test]
    fn the_codex_threshold_and_batch_are_conservative() {
        let images = ImagesConfig::default();
        assert!(images.codex_threshold > 0.0 && images.codex_threshold < 0.30);
        assert!(images.codex_batch > 0 && images.codex_batch <= 16);
    }

    /// Same mechanism as the scene mark, and it must not collide with any other
    /// mark the image chain writes.
    #[test]
    fn the_codex_mark_carries_the_prompt_version_and_collides_with_nothing() {
        let mut images = ImagesConfig::default();
        assert_eq!(images.codex_mark(), "codex_codex_1");
        images.codex_prompt_ver = CodexPromptVer("codex/2".into());
        assert_eq!(images.codex_mark(), "codex_codex_2");
        assert_ne!(images.codex_mark(), images.scene_mark());
        for source in ImageSource::ALL {
            assert_ne!(images.codex_mark(), source.as_str());
        }
        for pass in IMAGE_SECOND_PASSES {
            assert_ne!(images.codex_mark(), pass.mark);
        }
        let hostile = ImagesConfig {
            codex_prompt_ver: CodexPromptVer("codex: v2/alpha ".into()),
            ..ImagesConfig::default()
        };
        assert!(hostile
            .codex_mark()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_'));
    }

    /// A mark rides in a job subject of the form `{word_id}:{mark}`, so it must
    /// not contain anything that would make that ambiguous.
    #[test]
    fn the_scene_mark_survives_a_hostile_version_string() {
        let images = ImagesConfig {
            scene_prompt_ver: ScenePromptVer("scene: v2/alpha ".into()),
            ..ImagesConfig::default()
        };
        let mark = images.scene_mark();
        assert_eq!(mark, "sdxl_scene__v2_alpha_");
        assert!(mark.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    }

    /// The environment switch reads both ways, and refuses to guess.
    #[test]
    fn the_scene_mode_switch_is_a_boolean_in_both_directions() {
        for raw in ["1", "true", "TRUE", " yes ", "on"] {
            assert_eq!(parse_flag(raw), Some(true), "{raw:?}");
        }
        for raw in ["0", "false", "No", "off"] {
            assert_eq!(parse_flag(raw), Some(false), "{raw:?}");
        }
        // Not "probably true": an operator who typoed gets the configured
        // value and a warning, not a GPU pass they did not ask for.
        for raw in ["", "  ", "maybe", "2"] {
            assert_eq!(parse_flag(raw), None, "{raw:?}");
        }
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
                "wikimedia",
                "wikipedia",
                "openverse",
                "tatoeba",
                "wordnet",
                "exam_corpus",
                "unsplash",
                "pexels",
                "pixabay",
                "sdxl",
                "clip",
                "codex"
            ]
        );
        assert!(described.iter().any(|(_, state)| state == "disabled"));
        // The keyless sources report an endpoint, never "disabled".
        for name in [
            "freedict",
            "wiktionary",
            "wikimedia",
            "wikipedia",
            "openverse",
            "tatoeba",
        ] {
            let (_, state) = described.iter().find(|(n, _)| *n == name).unwrap();
            assert!(state.starts_with("https://"), "{name} reported {state}");
        }
    }
}
