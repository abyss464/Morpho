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

/// Working-database schema version stamped into `PRAGMA user_version`.
pub const SCHEMA_USER_VERSION: i32 = 1;
