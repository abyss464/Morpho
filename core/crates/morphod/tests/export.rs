//! End-to-end export tests over a synthetic working database.
//!
//! These rows are test vectors, not product content: they exist to pin the
//! graph, the dependency-closed cut and the bit-for-bit reproducibility of the
//! release bundle. Nothing here touches the network, and nothing here stands in
//! for real fetched content — the live sources are exercised by a manual run.

use std::collections::BTreeSet;
use std::path::Path;

use morpho_domain::event::Actor;
use morpho_domain::hash::text_hash;
use morpho_domain::tts::TtsConfig;
use morpho_domain::types::{
    CreatedBy, DefinitionSource, ExampleSource, ImageSource, MediaKind, Role, SelectedBy, SlotRef,
    TtsKind,
};
use morpho_export::{ExportSettings, HoldbackReport};
use morpho_reconcile::{EngineContext, PlanParams, SourceSet, TextPipeline};
use morpho_store::ops::{
    BindDistractors, CreateWord, DistractorBinding, IngestDefinitions, IngestExamples,
    IngestImages, MediaRegistration, RecordTtsAsset,
};
use morpho_store::{MediaStore, Store, StoreConfig, WriteOp};

struct Fixture {
    dir: tempfile::TempDir,
    store: Store,
    media: MediaStore,
    context: EngineContext,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(StoreConfig::new(dir.path().join("working.db"))).unwrap();
    let media = MediaStore::new(dir.path());
    let sources = SourceSet::load(Default::default(), Default::default()).unwrap();
    let context = EngineContext::new(sources, media.clone()).with_plan_params(PlanParams {
        group_min: 2,
        group_max: 4,
    });
    Fixture {
        dir,
        store,
        media,
        context,
    }
}

impl Fixture {
    fn settings(&self) -> ExportSettings {
        let pipeline = TextPipeline::default();
        ExportSettings {
            tts: TtsConfig::default(),
            tokenizer_ver: pipeline.tokenizer_ver().to_string(),
            lemmatizer_ver: pipeline.lemmatizer_ver().to_string(),
            data_dir: self.dir.path().to_path_buf(),
            exporter: "morphod-test".to_string(),
        }
    }

    async fn word(&self, lemma: &str, role: Role, rank: i64) -> i64 {
        let mut request = CreateWord::new(lemma, role, CreatedBy::Import);
        request.frequency_rank = Some(rank);
        request.phonetic = Some(format!("/{lemma}/"));
        self.store
            .write(Actor::Cli, WriteOp::CreateWord(request))
            .await
            .unwrap()
            .result
            .word_id()
            .unwrap()
    }

    /// Give a word a complete, approved asset set.
    async fn complete(&self, word_id: i64, lemma: &str, definition: &str) {
        self.definition(word_id, definition).await;
        self.example(word_id, lemma).await;
        self.image(word_id, lemma).await;
        self.audio(word_id, lemma, definition).await;
    }

    async fn definition(&self, word_id: i64, text: &str) {
        self.store
            .write(
                Actor::Worker(morpho_domain::JobKind::FetchDefinitions),
                WriteOp::IngestDefinitions(IngestDefinitions {
                    word_id,
                    source: DefinitionSource::Freedict,
                    definitions: vec![morpho_domain::FetchedDefinition {
                        pos: morpho_domain::Pos::Adj,
                        text: text.to_string(),
                        source_ref: None,
                    }],
                    phonetic: None,
                }),
            )
            .await
            .unwrap();

        let hash = text_hash(text);
        let cand: i64 = self
            .store
            .read(move |conn| {
                Ok(conn.query_row(
                    "SELECT def_cand_id FROM definition_candidates
                     WHERE word_id = ?1 AND text_hash = ?2",
                    rusqlite::params![word_id, hash],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();

        let slot = SlotRef::Definition {
            word_id,
            pos: "adj".to_string(),
        };
        self.store
            .write(
                Actor::Reconciler,
                WriteOp::select(slot.clone(), cand, SelectedBy::Auto),
            )
            .await
            .unwrap();
        self.store
            .write(
                Actor::Reconciler,
                WriteOp::SetPrimarySense {
                    word_id,
                    pos: "adj".to_string(),
                },
            )
            .await
            .unwrap();
        self.store
            .write(Actor::admin("abyss"), WriteOp::approve(slot))
            .await
            .unwrap();

        // Extraction freshness is an export gate, so record it for real.
        let pipeline = TextPipeline::default();
        self.store
            .write(
                Actor::Reconciler,
                WriteOp::record_extraction(
                    cand,
                    text_hash(text),
                    pipeline.input_hash(&text_hash(text)),
                    pipeline.tokenizer_ver().to_string(),
                    pipeline.lemmatizer_ver().to_string(),
                    pipeline.extract(text),
                ),
            )
            .await
            .unwrap();
    }

    async fn example(&self, word_id: i64, lemma: &str) {
        let text = format!("A {lemma} thing.");
        let start = 2;
        let end = start + lemma.len() as i64;
        self.store
            .write(
                Actor::Worker(morpho_domain::JobKind::FetchExamples),
                WriteOp::IngestExamples(IngestExamples {
                    word_id,
                    source: ExampleSource::ExamCorpus,
                    examples: vec![morpho_domain::FetchedExample {
                        text: text.clone(),
                        hl_start: start,
                        hl_end: end,
                        source_ref: None,
                    }],
                }),
            )
            .await
            .unwrap();

        let hash = text_hash(&text);
        let cand: i64 = self
            .store
            .read(move |conn| {
                Ok(conn.query_row(
                    "SELECT ex_cand_id FROM example_candidates WHERE word_id = ?1 AND text_hash = ?2",
                    rusqlite::params![word_id, hash],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        let slot = SlotRef::Example { word_id, slot: 1 };
        self.store
            .write(
                Actor::Reconciler,
                WriteOp::select(slot.clone(), cand, SelectedBy::Auto),
            )
            .await
            .unwrap();
        self.store
            .write(Actor::admin("abyss"), WriteOp::approve(slot))
            .await
            .unwrap();
    }

    async fn image(&self, word_id: i64, lemma: &str) {
        // Distinct bytes per word so the content addresses differ.
        let stored = self
            .media
            .put_bytes(format!("RIFF-webp-{lemma}").as_bytes(), MediaKind::Image)
            .unwrap();
        self.store
            .write(
                Actor::Worker(morpho_domain::JobKind::FetchImages),
                WriteOp::IngestImages(IngestImages {
                    word_id,
                    source: ImageSource::Unsplash,
                    images: vec![morpho_domain::FetchedImage {
                        file_hash: stored.file_hash.clone(),
                        width: Some(768),
                        height: Some(576),
                        source: ImageSource::Unsplash,
                        source_ref: Some(format!("unsplash:{lemma}")),
                        license: Some("Unsplash License".into()),
                        query_used: Some(lemma.to_string()),
                    }],
                    media: vec![MediaRegistration {
                        file_hash: stored.file_hash.clone(),
                        kind: MediaKind::Image,
                        rel_path: stored.rel_path,
                        bytes: stored.bytes,
                    }],
                    mark_source: None,
                }),
            )
            .await
            .unwrap();

        let cand: i64 = self
            .store
            .read(move |conn| {
                Ok(conn.query_row(
                    "SELECT img_cand_id FROM image_candidates WHERE word_id = ?1",
                    rusqlite::params![word_id],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        let slot = SlotRef::Image { word_id };
        self.store
            .write(
                Actor::Reconciler,
                WriteOp::select(slot.clone(), cand, SelectedBy::Auto),
            )
            .await
            .unwrap();
        self.store
            .write(Actor::admin("abyss"), WriteOp::approve(slot))
            .await
            .unwrap();
    }

    async fn audio(&self, _word_id: i64, lemma: &str, definition: &str) {
        let config = TtsConfig::default();
        let example = format!("A {lemma} thing.");
        for (kind, text) in [
            (TtsKind::Word, lemma.to_string()),
            (TtsKind::Definition, definition.to_string()),
            (TtsKind::Example, example),
        ] {
            let desired = config.desired(kind, &text);
            let stored = self
                .media
                .put_bytes(
                    format!("OggS-{}-{}", kind.as_str(), text).as_bytes(),
                    MediaKind::Audio,
                )
                .unwrap();
            self.store
                .write(
                    Actor::Worker(morpho_domain::JobKind::SynthTts),
                    WriteOp::RecordTtsAsset(RecordTtsAsset {
                        input_hash: desired.input_hash,
                        text_hash: text_hash(&text),
                        text,
                        kind,
                        voice: config.voice.clone(),
                        engine: config.engine.clone(),
                        engine_ver: "edge-tts/7.0.0".into(),
                        params_json: desired.params_json,
                        media: Some(MediaRegistration {
                            file_hash: stored.file_hash,
                            kind: MediaKind::Audio,
                            rel_path: stored.rel_path,
                            bytes: stored.bytes,
                        }),
                        duration_ms: Some(1_000),
                    }),
                )
                .await
                .unwrap();
        }
    }

    /// Seed the base vocabulary a definition leans on. Without it every
    /// definition would report `oos_pending`, which is correct but not what
    /// these tests are about.
    async fn base_words(&self, lemmas: &[&str]) {
        for lemma in lemmas {
            self.word(lemma, Role::Base, 1).await;
        }
    }

    async fn bind_distractors(&self, word_id: i64, others: [i64; 3]) {
        self.store
            .write(
                Actor::Reconciler,
                WriteOp::BindDistractors(BindDistractors {
                    bindings: vec![DistractorBinding {
                        word_id,
                        ranks: vec![(1, others[0]), (2, others[1]), (3, others[2])],
                    }],
                    algo_ver: "distractor/1".into(),
                }),
            )
            .await
            .unwrap();
    }

    /// Run the local sweep until it settles, so plan and readiness are real.
    async fn converge(&self) {
        let clocks = morpho_reconcile::stages::SweepClocks::new();
        for _ in 0..6 {
            let stats = morpho_reconcile::stages::run(&self.store, &self.context, &clocks)
                .await
                .unwrap();
            if stats.is_quiet() {
                break;
            }
        }
    }

    async fn preview(&self) -> HoldbackReport {
        morpho_export::preview(&self.store, &self.settings())
            .await
            .unwrap()
    }
}

/// Four mutually-distracting words, all complete. Four so each can name three
/// others without naming itself.
async fn four_complete_words(f: &Fixture) -> Vec<i64> {
    f.base_words(&["to", "change", "in", "a", "way"]).await;
    let lemmas = ["adapt", "adopt", "adept", "adapter"];
    let mut ids = Vec::new();
    for (index, lemma) in lemmas.iter().enumerate() {
        let id = f.word(lemma, Role::Target, (index as i64 + 1) * 100).await;
        f.complete(id, lemma, &format!("to change {lemma} in a way"))
            .await;
        ids.push(id);
    }
    for (index, id) in ids.iter().enumerate() {
        let others: Vec<i64> = ids
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, id)| *id)
            .collect();
        f.bind_distractors(*id, [others[0], others[1], others[2]])
            .await;
    }
    f.converge().await;
    ids
}

/// The same four words, except that the first one's definition leans on
/// `outsider` — a word that exists in the lexicon and has nothing else.
///
/// This is the shape ruling #18a is about: one dead word inside one definition,
/// dragging four finished words off the boat behind it.
async fn four_words_leaning_on(f: &Fixture, outsider: &str, role: Role) -> (Vec<i64>, i64) {
    f.base_words(&["to", "change", "in", "a", "way"]).await;
    let dead = f.word(outsider, role, 9_000).await;

    let lemmas = ["adapt", "adopt", "adept", "adapter"];
    let mut ids = Vec::new();
    for (index, lemma) in lemmas.iter().enumerate() {
        let id = f.word(lemma, Role::Target, (index as i64 + 1) * 100).await;
        let subject = if index == 0 { outsider } else { lemma };
        f.complete(id, lemma, &format!("to change {subject} in a way"))
            .await;
        ids.push(id);
    }
    for (index, id) in ids.iter().enumerate() {
        let others: Vec<i64> = ids
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, id)| *id)
            .collect();
        f.bind_distractors(*id, [others[0], others[1], others[2]])
            .await;
    }
    f.converge().await;
    (ids, dead)
}

fn read_db<T: rusqlite::types::FromSql>(path: &Path, sql: &str) -> T {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

fn rows_db(path: &Path, sql: &str) -> Vec<(i64, String, String)> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn.prepare(sql).unwrap();
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    rows
}

#[tokio::test]
async fn a_complete_lexicon_exports_every_word() {
    let f = fixture();
    let ids = four_complete_words(&f).await;

    let report = f.preview().await;
    assert_eq!(report.shippable_count, ids.len(), "{report:#?}");
    assert_eq!(report.exportable_count, ids.len());
    assert_eq!(report.excluded_count, 0);
    assert!(report.gates_pass, "{:#?}", report.gate_failures);

    let out = f.dir.path().join("release-1");
    let (written, _) = morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap();

    assert_eq!(written.word_count, 4);
    // 4 images + 4 word + 4 definition + 4 example audio files.
    assert_eq!(written.media_hashes.len(), 16);
    assert!(out.join("release.db").is_file());
    assert!(out.join("manifest.json").is_file());
}

#[tokio::test]
async fn the_release_db_matches_the_contract_shape() {
    let f = fixture();
    four_complete_words(&f).await;
    let out = f.dir.path().join("release-1");
    morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap();
    let db = out.join("release.db");

    // Reproducibility pragmas (README Part 5).
    assert_eq!(read_db::<i64>(&db, "PRAGMA page_size"), 4096);
    let journal: String = read_db(&db, "PRAGMA journal_mode");
    assert_eq!(journal.to_lowercase(), "delete");
    assert_eq!(read_db::<i64>(&db, "PRAGMA freelist_count"), 0, "VACUUMed");

    // Every contract table exists and is populated.
    for (table, expected) in [
        ("words", 4),
        ("senses", 4),
        ("examples", 4),
        ("groups", 1),
        ("distractors", 12),
    ] {
        let count: i64 = read_db(&db, &format!("SELECT COUNT(*) FROM {table}"));
        assert_eq!(count, expected, "{table}");
    }

    // Media columns point at content-addressed bundle paths, and the files are
    // actually there.
    let conn = rusqlite::Connection::open(&db).unwrap();
    let mut stmt = conn
        .prepare("SELECT image_file, word_audio_file FROM words ORDER BY word_id")
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for (image, audio) in rows {
        assert!(
            image.starts_with("img/") && image.ends_with(".webp"),
            "{image}"
        );
        assert!(
            audio.starts_with("audio/") && audio.ends_with(".ogg"),
            "{audio}"
        );
        assert!(
            out.join(&image).is_file(),
            "{image} missing from the bundle"
        );
        assert!(
            out.join(&audio).is_file(),
            "{audio} missing from the bundle"
        );
    }

    // Exactly one primary sense per word.
    let primaries: i64 = read_db(&db, "SELECT COUNT(*) FROM senses WHERE is_primary = 1");
    assert_eq!(primaries, 4);

    // meta carries the contract's four keys and no wall clock.
    let mut stmt = conn
        .prepare("SELECT key, value FROM meta ORDER BY key")
        .unwrap();
    let meta: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let keys: Vec<&str> = meta.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        keys,
        vec!["content_version", "exported_at", "plan_id", "schema_ver"]
    );
    let exported_at = &meta[1].1;
    assert_eq!(
        exported_at.len(),
        10,
        "a date, not a timestamp: {exported_at}"
    );
}

#[tokio::test]
async fn two_exports_of_the_same_state_are_byte_identical() {
    let f = fixture();
    four_complete_words(&f).await;

    let first = f.dir.path().join("release-a");
    let second = f.dir.path().join("release-b");
    let (a, _) = morpho_export::export(&f.store, &f.settings(), &first, "abyss", None)
        .await
        .unwrap();
    let (b, _) = morpho_export::export(&f.store, &f.settings(), &second, "abyss", None)
        .await
        .unwrap();

    assert_eq!(a.content_hash, b.content_hash);
    assert_eq!(a.content_version, b.content_version);
    assert_eq!(a.db_file_hash, b.db_file_hash);
    assert_eq!(
        std::fs::read(first.join("release.db")).unwrap(),
        std::fs::read(second.join("release.db")).unwrap(),
        "release.db must be byte-reproducible"
    );
    assert_eq!(
        std::fs::read_to_string(first.join("manifest.json")).unwrap(),
        std::fs::read_to_string(second.join("manifest.json")).unwrap()
    );
}

#[tokio::test]
async fn the_version_string_follows_the_whitepaper_format() {
    let f = fixture();
    four_complete_words(&f).await;
    let out = f.dir.path().join("release-1");
    let (written, _) = morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap();

    let (date, hash) = written
        .content_version
        .split_once('+')
        .expect("YYYY.MM.DD+hash");
    assert_eq!(date.len(), 10, "{date}");
    assert_eq!(date.matches('.').count(), 2);
    assert_eq!(hash.len(), 8);
    assert!(written.content_hash.starts_with(hash));
}

#[tokio::test]
async fn the_manifest_lists_every_file_with_its_real_hash() {
    let f = fixture();
    four_complete_words(&f).await;
    let out = f.dir.path().join("release-1");
    let (written, _) = morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap();

    let manifest: morpho_export::Manifest =
        serde_json::from_str(&std::fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.content_version, written.content_version);
    assert_eq!(manifest.word_count, 4);
    assert_eq!(manifest.media_count, 16);
    // 16 media files plus release.db.
    assert_eq!(manifest.files.len(), 17);
    assert!(manifest.exporter.contains("morphod"));

    let mut paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
    let sorted = {
        let mut copy = paths.clone();
        copy.sort_unstable();
        copy
    };
    assert_eq!(paths, sorted, "manifest entries are sorted by path");
    paths.dedup();
    assert_eq!(paths.len(), manifest.files.len(), "no duplicate entries");

    for entry in &manifest.files {
        let bytes = std::fs::read(out.join(&entry.path)).unwrap();
        assert_eq!(bytes.len() as u64, entry.bytes, "{}", entry.path);
        assert_eq!(
            morpho_domain::hash::file_hash(&bytes),
            entry.file_hash,
            "{}",
            entry.path
        );
    }
    assert_eq!(
        manifest.total_bytes,
        manifest.files.iter().map(|f| f.bytes).sum::<u64>()
    );
}

#[tokio::test]
async fn a_release_pins_its_media_against_gc() {
    let f = fixture();
    four_complete_words(&f).await;
    let out = f.dir.path().join("release-1");
    morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap();

    let pinned: i64 = f
        .store
        .read(|conn| {
            Ok(
                conn.query_row("SELECT COUNT(*) FROM release_manifests", [], |row| {
                    row.get(0)
                })?,
            )
        })
        .await
        .unwrap();
    assert_eq!(pinned, 16);

    let releases: i64 = f
        .store
        .read(|conn| Ok(conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0))?))
        .await
        .unwrap();
    assert_eq!(releases, 1);
}

/// Ruling #15: the exporter writes `releases.word_count`; nothing has to
/// reconstruct it from the audit log afterwards.
#[tokio::test]
async fn the_release_row_records_its_word_count() {
    let f = fixture();
    four_complete_words(&f).await;
    let out = f.dir.path().join("release-1");
    let (written, _) = morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap();
    assert_eq!(written.word_count, 4);

    let stored: i64 = f
        .store
        .read(|conn| Ok(conn.query_row("SELECT word_count FROM releases", [], |row| row.get(0))?))
        .await
        .unwrap();
    assert_eq!(stored as usize, written.word_count);
}

/// Ruling #13: the word's `tts_failed` blocker and the per-text
/// `TtsStatusView.status` are one verdict, computed once.
#[tokio::test]
async fn a_dead_tts_job_agrees_across_the_blocker_and_the_view() {
    use morpho_domain::job::{JobKey, JobKind, JobStatus, RateKey, SubjectRef};
    use morpho_store::ops::UpsertJobState;

    let f = fixture();
    let word = f.word("serene", Role::Target, 4_602).await;
    let config = TtsConfig::default();
    // No `tts_assets` row at all: the dead job is the only record of failure.
    let input_hash = config.input_hash(TtsKind::Word, "serene");
    f.store
        .write(
            Actor::Worker(JobKind::SynthTts),
            WriteOp::UpsertJobState(UpsertJobState {
                key: JobKey::new(JobKind::SynthTts, SubjectRef::tts_input(input_hash.clone())),
                rate_key: RateKey::EdgeTts,
                status: JobStatus::Dead,
                attempts: 5,
                next_retry_at: None,
                last_error: Some("edge-tts rejected the voice".into()),
            }),
        )
        .await
        .unwrap();

    f.converge().await;

    let probe = input_hash.clone();
    let (blockers, statuses) = f
        .store
        .read(move |conn| {
            let raw: String = conn.query_row(
                "SELECT blockers FROM words WHERE word_id = ?1",
                rusqlite::params![word],
                |row| row.get(0),
            )?;
            let detail = morpho_api::queries::word_detail(conn, word, &TtsConfig::default())?;
            let statuses: Vec<String> = detail
                .tts
                .iter()
                .filter(|view| view.input_hash == probe)
                .map(|view| view.status.clone())
                .collect();
            Ok((morpho_domain::blocker::parse_blockers(&raw), statuses))
        })
        .await
        .unwrap();

    assert_eq!(statuses, vec!["failed".to_string()]);
    assert!(blockers.contains(&"tts_failed".to_string()), "{blockers:?}");
    assert!(
        !blockers.contains(&"tts_missing".to_string()),
        "the lemma is the only desired clip, and it failed: {blockers:?}"
    );
}

#[tokio::test]
async fn a_broken_word_pulls_its_dependents_out_and_the_report_says_why() {
    let f = fixture();
    let ids = four_complete_words(&f).await;

    // Un-approve one word's image: it stops being shippable, and everything
    // that names it as a distractor follows it out.
    f.store
        .write(
            Actor::admin("abyss"),
            WriteOp::unapprove(SlotRef::Image { word_id: ids[0] }),
        )
        .await
        .unwrap();
    f.converge().await;

    let report = f.preview().await;
    assert_eq!(report.shippable_count, 3);
    assert_eq!(
        report.exportable_count, 0,
        "the other three all distract towards the broken one"
    );
    assert_eq!(report.excluded_count, 4);

    let broken = report
        .excluded
        .iter()
        .find(|entry| entry.word_id == ids[0])
        .unwrap();
    assert_eq!(broken.root_cause, "image_not_approved");
    assert_eq!(broken.impact_count, 3, "it blocks the other three");

    for entry in report.excluded.iter().filter(|e| e.word_id != ids[0]) {
        assert_eq!(entry.root_cause, "dependency_holdback");
        assert_eq!(entry.blocking_word_id, Some(ids[0]));
        assert_eq!(entry.blocking_lemma.as_deref(), Some("adapt"));
    }

    // Sorted by impact: the word to fix first is on top.
    assert_eq!(report.excluded[0].word_id, ids[0]);
}

#[tokio::test]
async fn every_held_back_word_is_explained() {
    let f = fixture();
    let ids = four_complete_words(&f).await;
    // A fifth word with nothing at all.
    let bare = f.word("copious", Role::Target, 5_000).await;
    f.converge().await;

    let report = f.preview().await;
    let explained: BTreeSet<i64> = report.excluded.iter().map(|e| e.word_id).collect();
    assert!(explained.contains(&bare));
    for entry in &report.excluded {
        assert!(!entry.lemma.is_empty());
        assert!(!entry.root_cause.is_empty());
        assert!(!entry.root_cause_detail.is_empty());
    }
    // The complete four still ship: the bare word is nobody's dependency.
    assert_eq!(report.exportable_count, ids.len());
}

#[tokio::test]
async fn an_empty_release_is_a_valid_outcome() {
    let f = fixture();
    // Words with no assets whatsoever — the state of a fresh import.
    for (index, lemma) in ["serene", "tranquil", "lucid"].iter().enumerate() {
        f.word(lemma, Role::Target, (index as i64 + 1) * 100).await;
    }
    f.converge().await;

    let report = f.preview().await;
    assert_eq!(report.shippable_count, 0);
    assert_eq!(report.exportable_count, 0);
    assert_eq!(report.excluded_count, 3);
    assert!(report.gates_pass, "an empty cut breaks no invariant");

    let out = f.dir.path().join("release-empty");
    let (written, _) = morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap();
    assert_eq!(written.word_count, 0);
    assert!(written.media_hashes.is_empty());

    let db = out.join("release.db");
    assert_eq!(read_db::<i64>(&db, "SELECT COUNT(*) FROM words"), 0);
    // Even an empty release carries its identity.
    assert_eq!(
        read_db::<String>(&db, "SELECT value FROM meta WHERE key = 'content_version'"),
        written.content_version
    );
}

#[tokio::test]
async fn refusing_to_overwrite_an_existing_bundle() {
    let f = fixture();
    four_complete_words(&f).await;
    let out = f.dir.path().join("release-1");
    morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap();
    let err = morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, morpho_export::ExportError::OutputExists(_)),
        "{err}"
    );
}

#[tokio::test]
async fn the_plan_orders_dependencies_before_their_dependents() {
    let f = fixture();
    // benevolent's definition uses generous; generous's uses kind.
    f.base_words(&["gentle", "caring", "and", "giving"]).await;
    let kind = f.word("kind", Role::Target, 100).await;
    let generous = f.word("generous", Role::Target, 2_180).await;
    let benevolent = f.word("benevolent", Role::Target, 4_312).await;
    f.definition(kind, "gentle and caring").await;
    f.definition(generous, "kind and giving").await;
    f.definition(benevolent, "generous and gentle").await;
    f.converge().await;

    let orders: Vec<(i64, i64)> = f
        .store
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT pw.word_id, pw.learning_order FROM plan_words pw
                 JOIN plan_artifacts pa ON pa.plan_id = pw.plan_id AND pa.is_current = 1
                 ORDER BY pw.learning_order",
            )?;
            let rows = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
        .unwrap();
    let position = |word_id: i64| {
        orders
            .iter()
            .find(|(id, _)| *id == word_id)
            .map(|(_, order)| *order)
            .unwrap()
    };
    assert!(position(kind) < position(generous), "{orders:?}");
    assert!(position(generous) < position(benevolent), "{orders:?}");
}

#[tokio::test]
async fn rebuilding_the_plan_from_unchanged_state_is_a_no_op() {
    let f = fixture();
    four_complete_words(&f).await;
    let before: (i64, String) = f
        .store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT plan_id, input_hash FROM plan_artifacts WHERE is_current = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .await
        .unwrap();

    f.converge().await;

    let after: (i64, String) = f
        .store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT plan_id, input_hash FROM plan_artifacts WHERE is_current = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(before, after, "an unchanged hash must not churn plan_id");

    let artifacts: i64 = f
        .store
        .read(|conn| {
            Ok(conn.query_row("SELECT COUNT(*) FROM plan_artifacts", [], |row| row.get(0))?)
        })
        .await
        .unwrap();
    assert_eq!(artifacts, 1);
}

#[tokio::test]
async fn readiness_lands_on_the_contract_blocker_vocabulary() {
    let f = fixture();
    let word = f.word("serene", Role::Target, 4_602).await;
    f.converge().await;

    let blockers: Vec<String> = f
        .store
        .read(move |conn| {
            let raw: String = conn.query_row(
                "SELECT blockers FROM words WHERE word_id = ?1",
                rusqlite::params![word],
                |row| row.get(0),
            )?;
            Ok(morpho_domain::blocker::parse_blockers(&raw))
        })
        .await
        .unwrap();

    for code in &blockers {
        assert!(
            code.parse::<morpho_domain::BlockerCode>().is_ok(),
            "{code} is outside the types.ts vocabulary"
        );
    }
    for expected in [
        "missing_definition",
        "missing_primary_sense",
        "missing_example",
        "missing_image",
        "tts_missing",
        "distractors_unbound",
    ] {
        assert!(blockers.contains(&expected.to_string()), "{blockers:?}");
    }
    // The word is in the plan, so `not_in_plan` must not fire.
    assert!(!blockers.contains(&"not_in_plan".to_string()));
}

#[tokio::test]
async fn a_complete_word_becomes_ready_and_core_ready() {
    let f = fixture();
    let ids = four_complete_words(&f).await;

    let (ready, core_ready): (i64, i64) = f
        .store
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT ready, core_ready FROM words WHERE word_id = ?1",
                rusqlite::params![ids[0]],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!((ready, core_ready), (1, 1));
}

// ---------------------------------------------------------------------------
// Chinese gloss anchors (admin-api.md ruling #18a)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_dead_dependency_drags_four_finished_words_off_the_boat() {
    let f = fixture();
    let (ids, dead) = four_words_leaning_on(&f, "obscure", Role::Auxiliary).await;

    let report = f.preview().await;
    assert_eq!(
        report.exportable_count, 0,
        "one dead word inside one definition empties the release: {report:#?}"
    );
    let held = report
        .excluded
        .iter()
        .find(|entry| entry.word_id == ids[0])
        .unwrap();
    assert_eq!(held.root_cause, "dependency_holdback");
    assert_eq!(held.blocking_word_id, Some(dead));
}

#[tokio::test]
async fn a_gloss_anchor_frees_the_words_that_lean_on_it() {
    let f = fixture();
    let (ids, dead) = four_words_leaning_on(&f, "obscure", Role::Auxiliary).await;

    f.store
        .write(Actor::admin("abyss"), WriteOp::set_gloss(dead, "模糊的"))
        .await
        .unwrap();
    f.converge().await;

    let report = f.preview().await;
    assert_eq!(
        report.exportable_count,
        ids.len(),
        "the anchor terminates the chain: {report:#?}"
    );
    assert!(report.gates_pass, "{:#?}", report.gate_failures);
    assert_eq!(
        report.excluded_count, 0,
        "and the anchor itself is not a holdback — it was never a candidate"
    );

    let out = f.dir.path().join("release-gloss");
    let (written, _) = morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap();
    assert_eq!(written.word_count, 4);

    let db = out.join("release.db");
    assert_eq!(
        rows_db(
            &db,
            "SELECT word_id, word, zh_gloss FROM gloss_anchors ORDER BY word_id"
        ),
        vec![(dead, "obscure".to_string(), "模糊的".to_string())]
    );
    // The anchor is not a learnable word, and nobody distracts towards it.
    let learnable: i64 = read_db(&db, "SELECT COUNT(*) FROM words WHERE word = 'obscure'");
    assert_eq!(learnable, 0);
    let distractors: i64 = read_db(
        &db,
        "SELECT COUNT(*) FROM distractors WHERE distractor_word_id NOT IN (SELECT word_id FROM words)",
    );
    assert_eq!(distractors, 0, "a distractor is always a shipped word");
}

#[tokio::test]
async fn an_unreferenced_gloss_never_reaches_the_release() {
    let f = fixture();
    let (_, dead) = four_words_leaning_on(&f, "obscure", Role::Auxiliary).await;
    let orphan = f.word("perambulate", Role::Auxiliary, 9_500).await;

    f.store
        .write(
            Actor::admin("abyss"),
            WriteOp::Batch(vec![
                WriteOp::set_gloss(dead, "模糊的"),
                WriteOp::set_gloss(orphan, "漫步"),
            ]),
        )
        .await
        .unwrap();
    f.converge().await;

    let out = f.dir.path().join("release-orphan");
    morpho_export::export(&f.store, &f.settings(), &out, "abyss", None)
        .await
        .unwrap();

    let anchors = rows_db(
        &out.join("release.db"),
        "SELECT word_id, word, zh_gloss FROM gloss_anchors ORDER BY word_id",
    );
    let words: Vec<&str> = anchors.iter().map(|(_, word, _)| word.as_str()).collect();
    assert_eq!(
        words,
        vec!["obscure"],
        "only anchors a shipped definition actually mentions"
    );
}

#[tokio::test]
async fn clearing_a_gloss_puts_the_words_back_where_they_were() {
    let f = fixture();
    let (_, dead) = four_words_leaning_on(&f, "obscure", Role::Auxiliary).await;

    f.store
        .write(Actor::admin("abyss"), WriteOp::set_gloss(dead, "模糊的"))
        .await
        .unwrap();
    f.converge().await;
    assert_eq!(f.preview().await.exportable_count, 4);

    f.store
        .write(Actor::admin("abyss"), WriteOp::clear_gloss(dead))
        .await
        .unwrap();
    f.converge().await;
    assert_eq!(
        f.preview().await.exportable_count,
        0,
        "the word is a live dependency again"
    );
}

/// Ruling #18a stops at the readability chain: a distractor still has to be a
/// real shipped word, so anchoring one that is already bound holds its owner
/// back rather than quietly shipping a quiz with three options and two answers.
#[tokio::test]
async fn anchoring_a_bound_distractor_still_holds_its_owner_back() {
    let f = fixture();
    let ids = four_complete_words(&f).await;
    assert_eq!(f.preview().await.exportable_count, 4);

    f.store
        .write(Actor::admin("abyss"), WriteOp::set_gloss(ids[3], "适配器"))
        .await
        .unwrap();
    f.converge().await;

    let report = f.preview().await;
    assert_eq!(
        report.exportable_count, 0,
        "the other three all name it as a distractor: {report:#?}"
    );
    let held = report
        .excluded
        .iter()
        .find(|entry| entry.word_id == ids[0])
        .unwrap();
    assert_eq!(held.root_cause, "dependency_holdback");
    assert_eq!(held.blocking_word_id, Some(ids[3]));
}

/// The closure gate: a token that resolves to nothing at all is caught by the
/// exporter even when the readiness cache has gone stale behind its back.
#[tokio::test]
async fn a_definition_token_that_resolves_to_nothing_fails_the_gates() {
    let f = fixture();
    four_complete_words(&f).await;
    assert!(f.preview().await.gates_pass);

    // Pull a base word out from under the finished definitions without letting
    // the reconciler notice — the damage an offline edit would leave behind.
    // Hence a connection of its own: the store has no route for this on purpose.
    let offline = rusqlite::Connection::open(f.dir.path().join("working.db")).unwrap();
    offline
        .execute("DELETE FROM words WHERE lemma = 'way'", [])
        .unwrap();
    drop(offline);

    let report = f.preview().await;
    assert!(!report.gates_pass);
    assert!(
        report
            .gate_failures
            .iter()
            .any(|failure| failure.gate == "definition_token_resolves"),
        "{:#?}",
        report.gate_failures
    );
}
