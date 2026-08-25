//! blake3 hashing with unambiguous field framing.
//!
//! Rules from README Part 3 / the working-db header:
//!   * all hashes are lowercase-hex blake3;
//!   * inputs are canonicalized first ([`crate::canonicalize`]);
//!   * every hash mixes in the producing code's `algo_version`, so bumping a
//!     tool version invalidates exactly its own derived artifacts.
//!
//! Framing: each field is length-prefixed (u64 little endian) before its bytes,
//! so `["ab", "c"]` and `["a", "bc"]` can never collide. The `algo_version` is
//! just the first framed field, after a fixed domain tag.

use crate::canon::canonicalize;
use crate::version;

const DOMAIN_TAG: &[u8] = b"morpho.hash.v1";

/// Incremental builder for a framed blake3 hash.
#[derive(Clone)]
pub struct HashInput {
    hasher: blake3::Hasher,
}

impl HashInput {
    /// Start a hash for the given producing algorithm version.
    pub fn new(algo_version: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(DOMAIN_TAG);
        let mut this = Self { hasher };
        this.push(algo_version.as_bytes());
        this
    }

    fn push(&mut self, bytes: &[u8]) {
        self.hasher.update(&(bytes.len() as u64).to_le_bytes());
        self.hasher.update(bytes);
    }

    /// Append a raw byte field.
    #[must_use]
    pub fn field_bytes(mut self, bytes: impl AsRef<[u8]>) -> Self {
        self.push(bytes.as_ref());
        self
    }

    /// Append a text field verbatim (already canonical, e.g. another hash).
    #[must_use]
    pub fn field(self, text: impl AsRef<str>) -> Self {
        self.field_bytes(text.as_ref().as_bytes())
    }

    /// Append a text field after canonicalization.
    #[must_use]
    pub fn field_canonical(self, text: impl AsRef<str>) -> Self {
        self.field_bytes(canonicalize(text.as_ref()).as_bytes())
    }

    /// Finish and return lowercase hex.
    pub fn finish(self) -> String {
        self.hasher.finalize().to_hex().to_string()
    }
}

/// Hash a list of already-canonical fields under `algo_version`.
pub fn hash_fields(algo_version: &str, fields: &[&str]) -> String {
    let mut h = HashInput::new(algo_version);
    for f in fields {
        h = h.field(f);
    }
    h.finish()
}

/// `text_hash` for any candidate text (definition / example / TTS input).
///
/// The producing algorithm is the canonicalizer itself, so its version is what
/// gets mixed in.
pub fn text_hash(text: &str) -> String {
    HashInput::new(version::CANON_ALGO_VER)
        .field_canonical(text)
        .finish()
}

/// Content address of a media file: pure blake3 over the raw bytes.
///
/// Deliberately *not* algo-versioned — content addressing must stay stable
/// forever so the same image fetched from two sources dedupes to one file.
pub fn file_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// `def_extractions.input_hash` = blake3(text_hash ‖ tokenizer_ver ‖ lemmatizer_ver).
pub fn def_extraction_input_hash(
    text_hash: &str,
    tokenizer_ver: &str,
    lemmatizer_ver: &str,
) -> String {
    HashInput::new(version::EXTRACTION_ALGO_VER)
        .field(text_hash)
        .field(tokenizer_ver)
        .field(lemmatizer_ver)
        .finish()
}

/// `tts_assets.input_hash` = blake3(canonical(text) ‖ voice ‖ engine ‖ engine_ver ‖ params).
pub fn tts_input_hash(
    text: &str,
    voice: &str,
    engine: &str,
    engine_ver: &str,
    params_json: &str,
) -> String {
    HashInput::new(version::TTS_ALGO_VER)
        .field_canonical(text)
        .field(voice)
        .field(engine)
        .field(engine_ver)
        .field(params_json)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_lower_hex_64(s: &str) -> bool {
        s.len() == 64
            && s.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    }

    #[test]
    fn output_shape_is_lowercase_hex_64() {
        assert!(is_lower_hex_64(&text_hash("well meaning and kindly")));
        assert!(is_lower_hex_64(&text_hash("")));
        assert!(is_lower_hex_64(&file_hash(b"")));
        assert!(is_lower_hex_64(&def_extraction_input_hash("a", "b", "c")));
    }

    #[test]
    fn text_hash_is_deterministic() {
        let a = text_hash("well meaning and kindly");
        let b = text_hash("well meaning and kindly");
        assert_eq!(a, b);
    }

    #[test]
    fn text_hash_canonicalizes_first() {
        assert_eq!(
            text_hash("  well   meaning\tand kindly "),
            text_hash("well meaning and kindly")
        );
        assert_eq!(text_hash("cafe\u{0301}"), text_hash("caf\u{00e9}"));
    }

    #[test]
    fn text_hash_is_case_sensitive() {
        assert_ne!(text_hash("abandon"), text_hash("Abandon"));
    }

    #[test]
    fn algo_version_changes_the_hash() {
        let v1 = hash_fields("canon/1", &["same input"]);
        let v2 = hash_fields("canon/2", &["same input"]);
        assert_ne!(v1, v2);
    }

    #[test]
    fn field_framing_prevents_concatenation_collisions() {
        assert_ne!(
            hash_fields("x/1", &["ab", "c"]),
            hash_fields("x/1", &["a", "bc"])
        );
        assert_ne!(
            hash_fields("x/1", &["", "abc"]),
            hash_fields("x/1", &["abc"])
        );
        assert_ne!(hash_fields("x/1", &["a"]), hash_fields("x/1", &["a", ""]));
    }

    #[test]
    fn algo_version_cannot_be_confused_with_a_field() {
        // "ab" + field "c" must differ from "a" + field "bc".
        assert_ne!(hash_fields("ab", &["c"]), hash_fields("a", &["bc"]));
    }

    #[test]
    fn extraction_hash_tracks_every_input() {
        let base = def_extraction_input_hash("h", "tok/1", "lem/1");
        assert_ne!(base, def_extraction_input_hash("h2", "tok/1", "lem/1"));
        assert_ne!(base, def_extraction_input_hash("h", "tok/2", "lem/1"));
        assert_ne!(base, def_extraction_input_hash("h", "tok/1", "lem/2"));
        assert_eq!(base, def_extraction_input_hash("h", "tok/1", "lem/1"));
    }

    #[test]
    fn tts_hash_tracks_every_input() {
        let base = tts_input_hash("kind", "en-US-AriaNeural", "edge-tts", "7.0.0", "{}");
        assert_eq!(
            base,
            tts_input_hash("  kind ", "en-US-AriaNeural", "edge-tts", "7.0.0", "{}")
        );
        assert_ne!(
            base,
            tts_input_hash("kind", "en-GB-SoniaNeural", "edge-tts", "7.0.0", "{}")
        );
        assert_ne!(
            base,
            tts_input_hash("kind", "en-US-AriaNeural", "edge-tts", "7.0.1", "{}")
        );
        assert_ne!(
            base,
            tts_input_hash(
                "kind",
                "en-US-AriaNeural",
                "edge-tts",
                "7.0.0",
                "{\"rate\":\"+5%\"}"
            )
        );
    }

    #[test]
    fn file_hash_is_raw_content_address() {
        // Must equal plain blake3 of the bytes: no domain tag, no algo version,
        // so it stays stable forever and matches external tooling.
        assert_eq!(file_hash(b"abc"), blake3::hash(b"abc").to_hex().to_string());
        assert_ne!(file_hash(b"abc"), file_hash(b"abd"));
    }

    #[test]
    fn builder_matches_hash_fields() {
        let a = HashInput::new("x/1").field("one").field("two").finish();
        let b = hash_fields("x/1", &["one", "two"]);
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_field_differs_from_verbatim_field_when_input_is_dirty() {
        let dirty = "  a   b  ";
        let verbatim = HashInput::new("x/1").field(dirty).finish();
        let canonical = HashInput::new("x/1").field_canonical(dirty).finish();
        assert_ne!(verbatim, canonical);
        assert_eq!(canonical, HashInput::new("x/1").field("a b").finish());
    }
}
