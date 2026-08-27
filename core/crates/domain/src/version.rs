//! Algorithm versions mixed into derived-artifact hashes.
//!
//! Bumping any constant here is how a tool upgrade invalidates exactly its own
//! products (README Part 3, "计算出来的过时"). Never reuse an old version string
//! for different behavior.

/// Version of the canonicalizer that produces every `text_hash`.
pub const CANON_ALGO_VER: &str = "canon/1";

/// Version of the code that composes `def_extractions.input_hash`.
pub const EXTRACTION_ALGO_VER: &str = "def-extract/1";

/// Version of the code that composes `tts_assets.input_hash`.
pub const TTS_ALGO_VER: &str = "tts-input/1";

/// Version of the candidate scorer. Bumping it re-scores every candidate.
///
/// * `scorer/1` — readability + length + source prior, self-reference as a
///   0.12 component read off the cached extraction.
/// * `scorer/2` — self-reference is inflection-aware, read straight off the
///   candidate text, and multiplies the total down instead of docking it; a
///   sense-commonality prior ranks a source's earlier senses above its later
///   ones.
/// * `scorer/3` — an out-of-scope token multiplies a definition's total down
///   instead of only shading its readability component, so a definition the
///   learner cannot read can win a slot no clean candidate can fill and no
///   other.
/// * `scorer/4` — an image no longer carries a source prior: manual, stock,
///   keyless-provider and SDXL candidates are judged purely on resolution and
///   primary-sense match, never on which provider produced them. The two
///   remaining components are rescaled from their old 0.35/0.20 split so they
///   still sum to 1.0.
pub const SCORER_ALGO_VER: &str = "scorer/4";

/// Version of the plan builder (Tarjan → condensation → grouping).
pub const PLAN_ALGO_VER: &str = "plan/1";

/// Version of the code that composes `plan_artifacts.input_hash`.
pub const PLAN_INPUT_ALGO_VER: &str = "plan-input/1";

/// Version of the distractor binder. Written to `distractors.algo_ver`; the
/// table is deliberately exempt from staleness, so this is provenance only.
///
/// * `distractor/1` — nearest-by-DL distance, no stem or POS filtering.
/// * `distractor/2` — excludes morphological relatives (`shares_stem`) and
///   prefers same-POS candidates via the `(pos_mismatch, distance, …)` key.
pub const DISTRACTOR_ALGO_VER: &str = "distractor/2";

/// Version of the readiness/blocker evaluator.
pub const READINESS_ALGO_VER: &str = "readiness/1";

/// Version of the exporter, mixed into `releases.input_hash`.
pub const EXPORT_ALGO_VER: &str = "export/1";

/// `meta.schema_ver` written into every `release.db`.
///
/// * `1` — wave-1 shape: `words.image_file`.
/// * `2` — image migrated from `words` to `examples.image_file` (display_order=1).
pub const RELEASE_SCHEMA_VER: &str = "2";

/// Working-database schema version stamped into `PRAGMA user_version`.
///
/// * 1 — wave-1 shape.
/// * 2 — wave-2: `words.core_ready`, normative `rate_limits` seeds.
/// * 3 — wave-3: `releases.word_count` (admin-api.md ruling #15).
/// * 4 — wave-4: the `example_candidates.source` and `image_candidates.source`
///   CHECK unions widen for the keyless sources, and three lanes join the
///   `rate_limits` seeds (admin-api.md ruling #18).
/// * 5 — wave-7: `words.zh_gloss` and `words.zh_gloss_source`, the Chinese
///   gloss anchors that terminate the readability chain (admin-api.md ruling
///   #18a).
/// * 6 — wave-7: `oos_queue.status` gains `resolved_gloss`, so an out-of-scope
///   lemma can be closed by anchoring it instead of promoting or rewriting.
pub const SCHEMA_USER_VERSION: i32 = 6;

/// Version the embedded `docs/contracts/working-db.sql` describes.
///
/// The contract file is normative but only the conductor edits it, so it may
/// legitimately trail the code between a ruling and its contract sync. A
/// freshly created database is therefore stamped at *this* version and then
/// walked forward by the same migration ladder an existing database uses:
/// equal to [`SCHEMA_USER_VERSION`] (the settled state) the ladder runs
/// nothing, and behind it the ladder closes the gap. Either way "created from
/// the contract" and "migrated from wave 1" end up the same schema.
///
/// Raise it when the contract file gains what a migration already added. It may
/// never exceed [`SCHEMA_USER_VERSION`]; `store::schema` asserts that at compile
/// time.
///
/// It sits at 5 while the code is at 6: the contract already ships
/// `words.zh_gloss`, but its `oos_queue.status` CHECK has not yet been synced
/// with `resolved_gloss`, so the rung that widens it carries its own forward
/// DDL and still has to run over a database created straight from the file.
pub const CONTRACT_SCHEMA_VERSION: i32 = 6;
