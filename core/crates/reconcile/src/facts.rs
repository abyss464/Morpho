//! The per-pass fact set.
//!
//! Every rule needs roughly the same handful of joins — "which words already
//! have a definition", "which sources have been tried", "which jobs are dead".
//! Loading them once per pass and handing every rule the same immutable view is
//! what keeps a full derivation over 6 000 words a fixed number of index scans
//! instead of one query per word per rule.
//!
//! The whole set is read inside a single `Store::read`, so every rule sees one
//! consistent snapshot. Optimistic concurrency at write time handles the rest
//! (README Part 4 §"对账循环").

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::Connection;

use morpho_domain::job::{JobKey, JobStatus, SubjectRef};
use morpho_domain::tts::TtsConfig;
use morpho_domain::types::{DefinitionSource, ExampleSource, ImageSource, TtsKind};
use morpho_store::error::Result;
use morpho_store::queries::{self, JobStateRow, SourceFetchRow, TtsAssetRow, WordRow};

use morpho_store::ops::{FETCH_DEFINITIONS, FETCH_ETYMOLOGY, FETCH_EXAMPLES, FETCH_IMAGES};

/// Everything the rules read, loaded once.
#[derive(Debug, Default)]
pub struct Facts {
    /// `active_words`, ordered by `(frequency_rank, word_id)`.
    pub active: Vec<WordRow>,
    /// `(word_id, source)` → completion marker, per fetch kind.
    pub definitions_fetched: BTreeMap<(i64, String), SourceFetchRow>,
    pub examples_fetched: BTreeMap<(i64, String), SourceFetchRow>,
    pub etymology_fetched: BTreeMap<(i64, String), SourceFetchRow>,
    pub images_fetched: BTreeMap<(i64, String), SourceFetchRow>,
    /// Persisted failure state, keyed exactly like `job_state`.
    pub jobs: HashMap<JobKey, JobStateRow>,
    /// Words with at least one `available` candidate of each kind.
    pub words_with_definitions: HashSet<i64>,
    pub words_with_examples: HashSet<i64>,
    pub words_with_images: HashSet<i64>,
    /// Text of each word's selected primary sense, for image search queries
    /// and SDXL prompts.
    pub primary_gloss: HashMap<i64, String>,
    /// Desired TTS texts resolved against the current voice configuration.
    pub tts_desired: Vec<(TtsKind, String)>,
    /// Existing `tts_assets`, keyed by `input_hash`.
    pub tts_assets: HashMap<String, TtsAssetRow>,
}

impl Facts {
    /// Load the whole fact set from one read connection.
    pub fn load(conn: &Connection) -> Result<Self> {
        Ok(Self {
            active: queries::active_words(conn)?,
            definitions_fetched: queries::source_fetches(conn, FETCH_DEFINITIONS)?,
            examples_fetched: queries::source_fetches(conn, FETCH_EXAMPLES)?,
            etymology_fetched: queries::source_fetches(conn, FETCH_ETYMOLOGY)?,
            images_fetched: queries::source_fetches(conn, FETCH_IMAGES)?,
            jobs: queries::job_states(conn)?
                .into_iter()
                .map(|row| (row.key.clone(), row))
                .collect(),
            words_with_definitions: id_set(
                conn,
                "SELECT DISTINCT word_id FROM definition_candidates WHERE status = 'available'",
            )?,
            words_with_examples: id_set(
                conn,
                "SELECT DISTINCT word_id FROM example_candidates WHERE status = 'available'",
            )?,
            words_with_images: id_set(
                conn,
                "SELECT DISTINCT word_id FROM image_candidates WHERE status = 'available'",
            )?,
            primary_gloss: primary_glosses(conn)?,
            tts_desired: queries::tts_desired(conn)?,
            tts_assets: queries::tts_assets(conn)?,
        })
    }

    /// Status of one job subject, if a row exists.
    pub fn job_status(&self, key: &JobKey) -> Option<JobStatus> {
        self.jobs.get(key).map(|row| row.status)
    }

    /// Has this `(kind, word, source)` combination been tried to completion?
    ///
    /// "Completion" means a marker exists — a legitimate zero-result fetch
    /// counts, which is exactly why the marker exists (README Part 4 §"完成标记").
    pub fn fetched(&self, markers: &FetchKind, word_id: i64, source: &str) -> bool {
        self.markers(markers)
            .contains_key(&(word_id, source.to_string()))
    }

    /// A source is exhausted when it has been tried and produced nothing, or
    /// when its job is dead or waived.
    ///
    /// This is the trigger the fallback chains read: Wiktionary exhausted →
    /// Morfessor; all three photo libraries exhausted → SDXL (README Part 4).
    pub fn source_exhausted(
        &self,
        markers: &FetchKind,
        job_kind: morpho_domain::job::JobKind,
        word_id: i64,
        source: &str,
    ) -> bool {
        if let Some(marker) = self.markers(markers).get(&(word_id, source.to_string())) {
            if marker.result_count == 0 {
                return true;
            }
        }
        matches!(
            self.job_status(&JobKey::new(
                job_kind,
                SubjectRef::word_source(word_id, source)
            )),
            Some(JobStatus::Dead) | Some(JobStatus::Waived)
        )
    }

    /// When a source became exhausted for a word, if it is.
    pub fn exhausted_at(
        &self,
        markers: &FetchKind,
        job_kind: morpho_domain::job::JobKind,
        word_id: i64,
        source: &str,
    ) -> Option<String> {
        let marker = self
            .markers(markers)
            .get(&(word_id, source.to_string()))
            .filter(|m| m.result_count == 0)
            .map(|m| m.fetched_at.clone());
        let job = self
            .jobs
            .get(&JobKey::new(
                job_kind,
                SubjectRef::word_source(word_id, source),
            ))
            .filter(|row| matches!(row.status, JobStatus::Dead | JobStatus::Waived))
            .map(|row| row.updated_at.clone());
        match (marker, job) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn markers(&self, kind: &FetchKind) -> &BTreeMap<(i64, String), SourceFetchRow> {
        match kind {
            FetchKind::Definitions => &self.definitions_fetched,
            FetchKind::Examples => &self.examples_fetched,
            FetchKind::Etymology => &self.etymology_fetched,
            FetchKind::Images => &self.images_fetched,
        }
    }

    /// Desired TTS rows that have no `ready` asset for the current voice.
    pub fn missing_tts(&self, config: &TtsConfig) -> Vec<morpho_domain::tts::DesiredTts> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for (kind, text) in &self.tts_desired {
            let desired = config.desired(*kind, text);
            if !seen.insert(desired.input_hash.clone()) {
                continue;
            }
            let covered = self
                .tts_assets
                .get(&desired.input_hash)
                .is_some_and(|asset| asset.status == "ready");
            if !covered {
                out.push(desired);
            }
        }
        out
    }
}

/// Which `source_fetch.kind` a lookup is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchKind {
    Definitions,
    Examples,
    Etymology,
    Images,
}

impl FetchKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Definitions => FETCH_DEFINITIONS,
            Self::Examples => FETCH_EXAMPLES,
            Self::Etymology => FETCH_ETYMOLOGY,
            Self::Images => FETCH_IMAGES,
        }
    }
}

/// Source names as they appear in `source_fetch.source`.
pub const SOURCE_FREEDICT: &str = "freedict";
pub const SOURCE_WORDNET: &str = "wordnet";
pub const SOURCE_WIKTIONARY: &str = "wiktionary";
pub const SOURCE_MORFESSOR: &str = "morfessor";
pub const SOURCE_EXAM_CORPUS: &str = "exam_corpus";
pub const SOURCE_TATOEBA: &str = "tatoeba";

/// `source_fetch.source` for a definition source.
pub const fn definition_source_name(source: DefinitionSource) -> &'static str {
    match source {
        DefinitionSource::Freedict => SOURCE_FREEDICT,
        DefinitionSource::Wordnet => SOURCE_WORDNET,
        DefinitionSource::LlmRewrite => "llm_rewrite",
        DefinitionSource::Manual => "manual",
    }
}

/// `source_fetch.source` for an example source.
pub const fn example_source_name(source: ExampleSource) -> &'static str {
    match source {
        ExampleSource::ExamCorpus => SOURCE_EXAM_CORPUS,
        ExampleSource::Freedict => SOURCE_FREEDICT,
        ExampleSource::Tatoeba => SOURCE_TATOEBA,
        ExampleSource::Llm => "llm",
        ExampleSource::Manual => "manual",
    }
}

/// `source_fetch.source` for an image source.
pub const fn image_source_name(source: ImageSource) -> &'static str {
    match source {
        ImageSource::Unsplash => "unsplash",
        ImageSource::Pexels => "pexels",
        ImageSource::Pixabay => "pixabay",
        ImageSource::Wikimedia => "wikimedia",
        ImageSource::Openverse => "openverse",
        ImageSource::Sdxl => "sdxl",
        ImageSource::Manual => "manual",
    }
}

fn id_set(conn: &Connection, sql: &str) -> Result<HashSet<i64>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn primary_glosses(conn: &Connection) -> Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare(
        "SELECT ds.word_id, dc.text
         FROM definition_selections ds
         JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
         WHERE ds.is_primary = 1 AND ds.enabled = 1",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use morpho_domain::job::JobKind;
    use morpho_domain::types::TtsKind;

    fn facts_with_marker(source: &str, result_count: i64) -> Facts {
        let mut facts = Facts::default();
        let images = &mut facts.images_fetched;
        images.insert(
            (7, source.to_string()),
            SourceFetchRow {
                fetched_at: "2026-08-26T00:00:00.000Z".to_string(),
                result_count,
            },
        );
        facts
    }

    #[test]
    fn a_marker_means_the_source_was_tried() {
        let facts = facts_with_marker("unsplash", 3);
        assert!(facts.fetched(&FetchKind::Images, 7, "unsplash"));
        assert!(!facts.fetched(&FetchKind::Images, 7, "pexels"));
        assert!(!facts.fetched(&FetchKind::Images, 8, "unsplash"));
    }

    #[test]
    fn a_zero_result_marker_exhausts_the_source() {
        let empty = facts_with_marker("unsplash", 0);
        assert!(empty.source_exhausted(&FetchKind::Images, JobKind::FetchImages, 7, "unsplash"));
        let productive = facts_with_marker("unsplash", 3);
        assert!(!productive.source_exhausted(
            &FetchKind::Images,
            JobKind::FetchImages,
            7,
            "unsplash"
        ));
    }

    #[test]
    fn a_dead_or_waived_job_exhausts_the_source() {
        for status in [JobStatus::Dead, JobStatus::Waived] {
            let mut facts = Facts::default();
            let key = JobKey::new(JobKind::FetchImages, SubjectRef::word_source(7, "pexels"));
            facts.jobs.insert(
                key.clone(),
                JobStateRow {
                    key,
                    rate_key: morpho_domain::job::RateKey::Pexels,
                    status,
                    attempts: 8,
                    next_retry_at: None,
                    last_error: Some("boom".into()),
                    updated_at: "2026-08-26T01:00:00.000Z".into(),
                },
            );
            assert!(facts.source_exhausted(&FetchKind::Images, JobKind::FetchImages, 7, "pexels"));
        }
    }

    #[test]
    fn a_backoff_job_does_not_exhaust_the_source() {
        let mut facts = Facts::default();
        let key = JobKey::new(JobKind::FetchImages, SubjectRef::word_source(7, "pexels"));
        facts.jobs.insert(
            key.clone(),
            JobStateRow {
                key,
                rate_key: morpho_domain::job::RateKey::Pexels,
                status: JobStatus::Backoff,
                attempts: 2,
                next_retry_at: Some("2026-08-26T02:00:00.000Z".into()),
                last_error: None,
                updated_at: "2026-08-26T01:00:00.000Z".into(),
            },
        );
        assert!(!facts.source_exhausted(&FetchKind::Images, JobKind::FetchImages, 7, "pexels"));
    }

    #[test]
    fn exhaustion_timestamp_prefers_the_earlier_signal() {
        let mut facts = facts_with_marker("unsplash", 0);
        let key = JobKey::new(JobKind::FetchImages, SubjectRef::word_source(7, "unsplash"));
        facts.jobs.insert(
            key.clone(),
            JobStateRow {
                key,
                rate_key: morpho_domain::job::RateKey::Unsplash,
                status: JobStatus::Dead,
                attempts: 8,
                next_retry_at: None,
                last_error: None,
                updated_at: "2026-08-26T05:00:00.000Z".into(),
            },
        );
        assert_eq!(
            facts
                .exhausted_at(&FetchKind::Images, JobKind::FetchImages, 7, "unsplash")
                .as_deref(),
            Some("2026-08-26T00:00:00.000Z")
        );
    }

    #[test]
    fn missing_tts_skips_ready_assets_and_dedupes() {
        let config = TtsConfig::default();
        let mut facts = Facts {
            tts_desired: vec![
                (TtsKind::Word, "serene".into()),
                (TtsKind::Word, "serene".into()),
                (TtsKind::Definition, "calm and peaceful".into()),
            ],
            ..Facts::default()
        };
        let ready_hash = config.input_hash(TtsKind::Word, "serene");
        facts.tts_assets.insert(
            ready_hash.clone(),
            TtsAssetRow {
                input_hash: ready_hash,
                status: "ready".into(),
                file_hash: Some("abc".into()),
                duration_ms: Some(500),
                engine_ver: "edge-tts/7".into(),
            },
        );
        let missing = facts.missing_tts(&config);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].kind, TtsKind::Definition);
    }

    #[test]
    fn a_failed_asset_still_counts_as_missing_work() {
        let config = TtsConfig::default();
        let mut facts = Facts {
            tts_desired: vec![(TtsKind::Word, "serene".into())],
            ..Facts::default()
        };
        let hash = config.input_hash(TtsKind::Word, "serene");
        facts.tts_assets.insert(
            hash.clone(),
            TtsAssetRow {
                input_hash: hash,
                status: "failed".into(),
                file_hash: None,
                duration_ms: None,
                engine_ver: String::new(),
            },
        );
        assert_eq!(facts.missing_tts(&config).len(), 1);
    }

    /// `source_fetch.source` is a free-text column, so the only thing keeping
    /// the completion markers aligned with the candidate rows is that both
    /// spell a source the same way. These names *are* the enum's own strings.
    #[test]
    fn source_names_match_the_schema_check_constraints() {
        for source in DefinitionSource::ALL {
            assert_eq!(definition_source_name(*source), source.as_str());
        }
        for source in ExampleSource::ALL {
            assert_eq!(example_source_name(*source), source.as_str());
        }
        for source in ImageSource::ALL {
            assert_eq!(image_source_name(*source), source.as_str());
        }
        // Spot-check the wave-4 arrivals by hand as well.
        assert_eq!(example_source_name(ExampleSource::Tatoeba), "tatoeba");
        assert_eq!(image_source_name(ImageSource::Wikimedia), "wikimedia");
        assert_eq!(image_source_name(ImageSource::Openverse), "openverse");
    }
}
