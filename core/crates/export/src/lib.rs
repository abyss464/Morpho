//! The release exporter (README Part 5).
//!
//! Three steps, in order:
//!
//! 1. **Gate** every active word against the factory checklist. All of it is
//!    hash comparison already done by the reconciler — the exporter reads
//!    `core_ready`, the distractor bindings and extraction freshness, it does
//!    not re-derive them.
//! 2. **Cut** to the maximal dependency-closed subset, so no shipped word can
//!    reference a word that stayed behind, and produce the holdback report that
//!    tells a human which word to fix first.
//! 3. **Write** a byte-reproducible bundle and record it, pinning its media
//!    against garbage collection.
//!
//! An empty release is a legitimate outcome: with no image credentials and no
//! exam corpus, nothing passes the gates, the cut is empty, and the holdback
//! report explains every single word. That is the system working.

pub mod cut;
pub mod error;
pub mod model;
pub mod writer;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use morpho_domain::event::Actor;
use morpho_domain::tts::TtsConfig;
use morpho_store::ops::RecordRelease;
use morpho_store::{Store, WriteOp};

pub use cut::{CutNode, CutResult, Holdback, DEPENDENCY_HOLDBACK};
pub use error::{ExportError, ExportResult};
pub use model::{ExportPayload, ExportWord, GlossAnchor, STALE_EXTRACTION};
pub use writer::{Manifest, ManifestEntry, WrittenRelease};

/// One failed hard gate (`ExportGateFailure` in admin-ui/src/api/types.ts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateFailure {
    pub gate: String,
    pub message: String,
    pub word_id: Option<i64>,
    pub lemma: Option<String>,
}

/// `HoldbackReport` in admin-ui/src/api/types.ts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoldbackReport {
    pub plan_id: i64,
    pub shippable_count: usize,
    pub exportable_count: usize,
    pub excluded_count: usize,
    pub excluded: Vec<HoldbackEntry>,
    pub gates_pass: bool,
    pub gate_failures: Vec<GateFailure>,
}

/// One row of the holdback report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoldbackEntry {
    pub word_id: i64,
    pub lemma: String,
    pub role: String,
    pub root_cause: String,
    pub root_cause_detail: String,
    pub blocking_word_id: Option<i64>,
    pub blocking_lemma: Option<String>,
    pub impact_count: usize,
}

/// Settings one export run needs.
#[derive(Debug, Clone)]
pub struct ExportSettings {
    pub tts: TtsConfig,
    pub tokenizer_ver: String,
    pub lemmatizer_ver: String,
    /// Root of `data/`, used to read the media library.
    pub data_dir: std::path::PathBuf,
    /// Version string recorded in `manifest.json`.
    pub exporter: String,
}

/// Everything the preview and the export share.
struct Prepared {
    payload: ExportPayload,
    cut: CutResult,
    failures: Vec<GateFailure>,
}

async fn prepare(store: &Store, settings: &ExportSettings) -> ExportResult<Prepared> {
    let tts = settings.tts.clone();
    let tokenizer = settings.tokenizer_ver.clone();
    let lemmatizer = settings.lemmatizer_ver.clone();
    let payload = store
        .read(move |conn| model::load(conn, &tts, &tokenizer, &lemmatizer))
        .await?
        .ok_or(ExportError::NoPlan)?;

    let nodes: Vec<CutNode> = payload
        .words
        .iter()
        .map(|word| CutNode {
            word_id: word.word_id,
            shippable: word.shippable(),
            blockers: word.gate_blockers(),
            learning_order: word.learning_order,
        })
        .collect();

    // Both edge families, exactly as README Part 5 defines the closure graph.
    let mut edges = payload.dependency_edges.clone();
    edges.extend(
        payload
            .distractors
            .iter()
            .map(|(word_id, _, distractor)| (*word_id, *distractor)),
    );

    let cut = cut::compute(&nodes, &edges);
    let failures = validate(&payload, &cut);
    Ok(Prepared {
        payload,
        cut,
        failures,
    })
}

/// `GET /releases/preview`.
pub async fn preview(store: &Store, settings: &ExportSettings) -> ExportResult<HoldbackReport> {
    let prepared = prepare(store, settings).await?;
    Ok(report(&prepared))
}

fn report(prepared: &Prepared) -> HoldbackReport {
    let lemmas: std::collections::HashMap<i64, (&str, &str)> = prepared
        .payload
        .words
        .iter()
        .map(|word| (word.word_id, (word.lemma.as_str(), word.role.as_str())))
        .collect();

    HoldbackReport {
        plan_id: prepared.payload.plan_id,
        shippable_count: prepared.cut.shippable_count,
        exportable_count: prepared.cut.exportable.len(),
        excluded_count: prepared.cut.excluded.len(),
        excluded: prepared
            .cut
            .excluded
            .iter()
            .map(|entry| {
                let (lemma, role) = lemmas
                    .get(&entry.word_id)
                    .copied()
                    .unwrap_or(("<unknown>", "target"));
                HoldbackEntry {
                    word_id: entry.word_id,
                    lemma: lemma.to_string(),
                    role: role.to_string(),
                    root_cause: entry.root_cause.clone(),
                    root_cause_detail: entry.root_cause_detail.clone(),
                    blocking_lemma: entry
                        .blocking_word_id
                        .and_then(|id| lemmas.get(&id))
                        .map(|(lemma, _)| (*lemma).to_string()),
                    blocking_word_id: entry.blocking_word_id,
                    impact_count: entry.impact_count,
                }
            })
            .collect(),
        gates_pass: prepared.failures.is_empty(),
        gate_failures: prepared.failures.clone(),
    }
}

/// `POST /releases/export` and `morphod export`.
///
/// Writes the bundle into `out_dir` and records the release. Fails with
/// [`ExportError::GatesFailed`] when a hard validation gate rejects the cut.
pub async fn export(
    store: &Store,
    settings: &ExportSettings,
    out_dir: &std::path::Path,
    actor: &str,
    notes: Option<String>,
) -> ExportResult<(WrittenRelease, HoldbackReport)> {
    let prepared = prepare(store, settings).await?;
    if !prepared.failures.is_empty() {
        return Err(ExportError::GatesFailed(prepared.failures));
    }
    let holdback = report(&prepared);

    let rows = writer::rows_for(&prepared.payload, &prepared.cut.exportable);
    let written = writer::write_bundle(
        out_dir,
        &settings.data_dir,
        &prepared.payload,
        &rows,
        chrono::Utc::now().date_naive(),
        &settings.exporter,
    )?;

    store
        .write(
            Actor::admin(actor),
            WriteOp::RecordRelease(RecordRelease {
                version: written.content_version.clone(),
                plan_id: prepared.payload.plan_id,
                input_hash: written.content_hash.clone(),
                db_file_hash: written.db_file_hash.clone(),
                exported_by: actor.to_string(),
                notes,
                media_hashes: written.media_hashes.clone(),
                word_count: written.word_count,
            }),
        )
        .await?;

    Ok((written, holdback))
}

/// The hard validation gates (README Part 5, "导出校验器").
///
/// These are invariants of the *cut*, not per-word quality checks: if one of
/// them trips, the closure or the readiness computation has a bug, and shipping
/// would produce an APK that crashes on a missing asset. Failing loudly is the
/// point.
pub fn validate(payload: &ExportPayload, cut: &CutResult) -> Vec<GateFailure> {
    let mut failures = Vec::new();
    let exportable = &cut.exportable;
    let by_id: std::collections::HashMap<i64, &ExportWord> = payload
        .words
        .iter()
        .map(|word| (word.word_id, word))
        .collect();
    let lemma_of = |word_id: i64| by_id.get(&word_id).map(|w| w.lemma.clone());

    let mut fail = |gate: &str, message: String, word_id: Option<i64>| {
        failures.push(GateFailure {
            gate: gate.to_string(),
            message,
            lemma: word_id.and_then(lemma_of),
            word_id,
        });
    };

    for word_id in exportable {
        let Some(word) = by_id.get(word_id) else {
            fail(
                "word_present",
                format!("word {word_id} was cut in but is not in the lexicon"),
                Some(*word_id),
            );
            continue;
        };
        if !word.shippable() {
            fail(
                "shippable",
                format!("{} was cut in without passing its gates", word.lemma),
                Some(*word_id),
            );
        }
        if word.image_file_hash.is_none() {
            fail(
                "image_present",
                format!("{} has no selected image", word.lemma),
                Some(*word_id),
            );
        }
        if word.word_audio_hash.is_none() {
            fail(
                "word_audio_present",
                format!("{} has no word audio", word.lemma),
                Some(*word_id),
            );
        }
    }

    // Exactly one primary sense per exported word, and audio for every sense.
    let mut primaries: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for sense in &payload.senses {
        if !exportable.contains(&sense.word_id) {
            continue;
        }
        if sense.is_primary {
            *primaries.entry(sense.word_id).or_default() += 1;
        }
        if sense.audio_hash.is_none() {
            fail(
                "sense_audio_present",
                format!(
                    "{} has no audio for its {} sense",
                    lemma_of(sense.word_id).unwrap_or_default(),
                    sense.pos
                ),
                Some(sense.word_id),
            );
        }
    }
    for word_id in exportable {
        match primaries.get(word_id).copied().unwrap_or(0) {
            1 => {}
            count => fail(
                "one_primary_sense",
                format!(
                    "{} has {count} primary senses, expected exactly 1",
                    lemma_of(*word_id).unwrap_or_default()
                ),
                Some(*word_id),
            ),
        }
    }

    // Slot 1 example, with audio.
    let mut slot_one: HashSet<i64> = HashSet::new();
    for example in &payload.examples {
        if !exportable.contains(&example.word_id) {
            continue;
        }
        if example.display_order == 1 {
            slot_one.insert(example.word_id);
        }
        if example.audio_hash.is_none() {
            fail(
                "example_audio_present",
                format!(
                    "{} has no audio for example slot {}",
                    lemma_of(example.word_id).unwrap_or_default(),
                    example.display_order
                ),
                Some(example.word_id),
            );
        }
    }
    for word_id in exportable {
        if !slot_one.contains(word_id) {
            fail(
                "mode_one_example",
                format!(
                    "{} has no slot-1 example",
                    lemma_of(*word_id).unwrap_or_default()
                ),
                Some(*word_id),
            );
        }
    }

    // Three distractors, all resolving to exported words.
    let mut bound: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for (word_id, _, distractor) in &payload.distractors {
        if !exportable.contains(word_id) {
            continue;
        }
        if !exportable.contains(distractor) {
            fail(
                "distractor_resolves",
                format!(
                    "{}'s distractor {} is not in the release",
                    lemma_of(*word_id).unwrap_or_default(),
                    lemma_of(*distractor).unwrap_or_else(|| distractor.to_string())
                ),
                Some(*word_id),
            );
            continue;
        }
        *bound.entry(*word_id).or_default() += 1;
    }
    for word_id in exportable {
        let count = bound.get(word_id).copied().unwrap_or(0);
        if count != 3 {
            fail(
                "three_distractors",
                format!(
                    "{} has {count} usable distractors, expected 3",
                    lemma_of(*word_id).unwrap_or_default()
                ),
                Some(*word_id),
            );
        }
    }

    // Readability closure (ruling #18a): every token of a shipped definition
    // resolves to a base word, a shipped word, or a gloss anchor the release
    // carries. The three gates below are the three ways that can fail; between
    // them and `dependency_present` further down, nothing a learner can read is
    // left unexplained.
    for (word_id, lemma) in &payload.unresolved_tokens {
        if !exportable.contains(word_id) {
            continue;
        }
        fail(
            "definition_token_resolves",
            format!(
                "{}'s definition uses {lemma}, which is in no lexicon",
                lemma_of(*word_id).unwrap_or_default()
            ),
            Some(*word_id),
        );
    }
    let shipped_anchors: HashSet<i64> = rows_gloss_anchors(payload, cut);
    for (word_id, anchor) in &payload.anchor_refs {
        if !exportable.contains(word_id) || shipped_anchors.contains(anchor) {
            continue;
        }
        fail(
            "gloss_anchor_present",
            format!(
                "{} leans on gloss anchor {anchor}, which the release does not carry",
                lemma_of(*word_id).unwrap_or_default()
            ),
            Some(*word_id),
        );
    }

    // The topological invariant: every dependency is learned earlier, or in
    // the same group (an SCC pack).
    let order: std::collections::HashMap<i64, (i64, i64)> = payload
        .words
        .iter()
        .map(|word| (word.word_id, (word.learning_order, word.group_seq)))
        .collect();
    for (from, to) in &payload.dependency_edges {
        if !exportable.contains(from) {
            continue;
        }
        if !exportable.contains(to) {
            fail(
                "dependency_present",
                format!(
                    "{} depends on {}, which is not in the release",
                    lemma_of(*from).unwrap_or_default(),
                    lemma_of(*to).unwrap_or_else(|| to.to_string())
                ),
                Some(*from),
            );
            continue;
        }
        let (Some((from_order, from_group)), Some((to_order, to_group))) =
            (order.get(from), order.get(to))
        else {
            continue;
        };
        if from_group != to_group && to_order >= from_order {
            fail(
                "topological_order",
                format!(
                    "{} is learned before its dependency {}",
                    lemma_of(*from).unwrap_or_default(),
                    lemma_of(*to).unwrap_or_default()
                ),
                Some(*from),
            );
        }
    }

    failures
}

/// The anchor ids [`writer::rows_for`] will emit for this cut.
///
/// Asking the writer rather than recomputing the rule is what makes the gate
/// meaningful: it fails exactly when the bundle would ship an unexplained
/// token, not when a second copy of the selection logic disagrees.
fn rows_gloss_anchors(payload: &ExportPayload, cut: &CutResult) -> HashSet<i64> {
    writer::rows_for(payload, &cut.exportable)
        .gloss_anchors
        .iter()
        .map(|anchor| anchor.word_id)
        .collect()
}
