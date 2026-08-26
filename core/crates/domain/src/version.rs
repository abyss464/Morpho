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
pub const SCORER_ALGO_VER: &str = "scorer/1";

/// Version of the plan builder (Tarjan → condensation → grouping).
pub const PLAN_ALGO_VER: &str = "plan/1";

/// Version of the code that composes `plan_artifacts.input_hash`.
pub const PLAN_INPUT_ALGO_VER: &str = "plan-input/1";

/// Version of the distractor binder. Written to `distractors.algo_ver`; the
/// table is deliberately exempt from staleness, so this is provenance only.
pub const DISTRACTOR_ALGO_VER: &str = "distractor/1";

/// Version of the readiness/blocker evaluator.
pub const READINESS_ALGO_VER: &str = "readiness/1";

/// Version of the exporter, mixed into `releases.input_hash`.
pub const EXPORT_ALGO_VER: &str = "export/1";

/// `meta.schema_ver` written into every `release.db`.
pub const RELEASE_SCHEMA_VER: &str = "1";

/// Working-database schema version stamped into `PRAGMA user_version`.
///
/// * 1 — wave-1 shape.
/// * 2 — wave-2: `words.core_ready`, normative `rate_limits` seeds.
/// * 3 — wave-3: `releases.word_count` (admin-api.md ruling #15).
/// * 4 — wave-4: the `example_candidates.source` and `image_candidates.source`
///   CHECK unions widen for the keyless sources, and three lanes join the
///   `rate_limits` seeds (admin-api.md ruling #18).
pub const SCHEMA_USER_VERSION: i32 = 4;

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
pub const CONTRACT_SCHEMA_VERSION: i32 = 4;
