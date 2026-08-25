//! The TTS voice configuration, and the `input_hash` it produces.
//!
//! TTS is content-addressed by *what was synthesized* (README Part 3 §"派生 ·
//! TTS"): `blake3(canonical(text) ‖ voice ‖ engine ‖ engine_ver ‖ params)`.
//! Nothing is ever recomputed in place — changing the voice or a prosody knob
//! produces different hashes, so new rows appear, old rows lose their
//! references, and the orphans go to GC.
//!
//! `engine_ver` is deliberately part of the key but *not* known before the
//! first synthesis, so the desired-set diff pins it to the configured value and
//! the adapter's reported version is stored for the record. Configuring an
//! engine version that the adapter does not match is an operator decision, and
//! the console shows both.

use serde::{Deserialize, Serialize};

use crate::hash::tts_input_hash;
use crate::types::TtsKind;

/// Voice and prosody settings, straight from `morphod.toml [tts]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TtsConfig {
    /// edge-tts voice id.
    pub voice: String,
    /// edge-tts rate, e.g. `"+0%"`.
    pub rate: String,
    /// edge-tts pitch, e.g. `"+0Hz"`.
    pub pitch: String,
    /// edge-tts volume, e.g. `"+0%"` (adapter-protocol.md wave-2 ruling #2).
    pub volume: String,
    /// Bitrate for single-word audio. The dictation quiz needs clean phonemes,
    /// so the word track is richer than the prose tracks (README Part 5).
    pub word_bitrate_kbps: u32,
    /// Bitrate for definition and example audio.
    pub text_bitrate_kbps: u32,
    /// Engine identity mixed into every `input_hash`.
    pub engine: String,
    /// Engine version mixed into every `input_hash`.
    pub engine_ver: String,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            voice: "en-US-AriaNeural".to_string(),
            rate: "+0%".to_string(),
            pitch: "+0Hz".to_string(),
            volume: "+0%".to_string(),
            word_bitrate_kbps: 48,
            text_bitrate_kbps: 32,
            engine: "edge-tts".to_string(),
            engine_ver: "7".to_string(),
        }
    }
}

impl TtsConfig {
    /// Bitrate used for one kind of text.
    pub fn bitrate_kbps(&self, kind: TtsKind) -> u32 {
        match kind {
            TtsKind::Word => self.word_bitrate_kbps,
            TtsKind::Definition | TtsKind::Example => self.text_bitrate_kbps,
        }
    }

    /// `tts_assets.params_json`, serialized deterministically.
    ///
    /// Field order is fixed by this function, not by a map iteration, because
    /// the string is hashed: two configurations that differ only in map order
    /// must not produce two rows for the same audio.
    pub fn params_json(&self, kind: TtsKind) -> String {
        format!(
            r#"{{"bitrate_kbps":{},"format":"ogg_opus","pitch":{},"rate":{},"volume":{}}}"#,
            self.bitrate_kbps(kind),
            json_string(&self.pitch),
            json_string(&self.rate),
            json_string(&self.volume),
        )
    }

    /// The content address of one desired synthesis.
    pub fn input_hash(&self, kind: TtsKind, text: &str) -> String {
        tts_input_hash(
            text,
            &self.voice,
            &self.engine,
            &self.engine_ver,
            &self.params_json(kind),
        )
    }

    /// Describe one desired synthesis in full.
    pub fn desired(&self, kind: TtsKind, text: &str) -> DesiredTts {
        DesiredTts {
            kind,
            input_hash: self.input_hash(kind, text),
            text: text.to_string(),
            params_json: self.params_json(kind),
            bitrate_kbps: self.bitrate_kbps(kind),
        }
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

/// One row of the desired TTS set, resolved against the current configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesiredTts {
    pub kind: TtsKind,
    pub input_hash: String,
    pub text: String,
    pub params_json: String,
    pub bitrate_kbps: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_audio_gets_the_richer_bitrate() {
        let config = TtsConfig::default();
        assert_eq!(config.bitrate_kbps(TtsKind::Word), 48);
        assert_eq!(config.bitrate_kbps(TtsKind::Definition), 32);
        assert_eq!(config.bitrate_kbps(TtsKind::Example), 32);
    }

    #[test]
    fn params_json_is_valid_and_stable() {
        let config = TtsConfig::default();
        let raw = config.params_json(TtsKind::Word);
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid json");
        assert_eq!(parsed["bitrate_kbps"], 48);
        assert_eq!(parsed["rate"], "+0%");
        assert_eq!(parsed["volume"], "+0%");
        assert_eq!(raw, config.params_json(TtsKind::Word));
    }

    #[test]
    fn the_same_text_at_two_bitrates_is_two_assets() {
        let config = TtsConfig::default();
        assert_ne!(
            config.input_hash(TtsKind::Word, "serene"),
            config.input_hash(TtsKind::Definition, "serene")
        );
    }

    #[test]
    fn every_knob_moves_the_hash() {
        let base = TtsConfig::default();
        let hash = base.input_hash(TtsKind::Definition, "calm and peaceful");
        for mutate in [
            (|c: &mut TtsConfig| c.voice = "en-GB-SoniaNeural".into()) as fn(&mut TtsConfig),
            |c: &mut TtsConfig| c.rate = "+10%".into(),
            |c: &mut TtsConfig| c.pitch = "+5Hz".into(),
            |c: &mut TtsConfig| c.volume = "-10%".into(),
            |c: &mut TtsConfig| c.text_bitrate_kbps = 64,
            |c: &mut TtsConfig| c.engine_ver = "8".into(),
            |c: &mut TtsConfig| c.engine = "piper".into(),
        ] {
            let mut changed = base.clone();
            mutate(&mut changed);
            assert_ne!(
                hash,
                changed.input_hash(TtsKind::Definition, "calm and peaceful"),
                "a config change must produce a different asset"
            );
        }
    }

    #[test]
    fn canonicalization_happens_before_hashing() {
        let config = TtsConfig::default();
        assert_eq!(
            config.input_hash(TtsKind::Definition, "  calm   and peaceful "),
            config.input_hash(TtsKind::Definition, "calm and peaceful")
        );
    }

    #[test]
    fn desired_carries_everything_the_adapter_needs() {
        let config = TtsConfig::default();
        let desired = config.desired(TtsKind::Word, "serene");
        assert_eq!(desired.bitrate_kbps, 48);
        assert_eq!(desired.text, "serene");
        assert_eq!(
            desired.input_hash,
            config.input_hash(TtsKind::Word, "serene")
        );
    }
}
