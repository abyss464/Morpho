---
file: core/crates/domain/src/tts.rs
---

# TTS Voice Configuration

Configuration and content addressing for speech synthesis. TTS is addressed by "what was synthesized" — changing voice or prosody parameters produces a new hash, and old rows/lines are handed to GC once they lose their references.

## TtsConfig

Reads voice and prosody settings from `morphod.toml [tts]`. Fields: voice (voice ID), rate (speech rate), pitch (pitch), volume (volume), word_bitrate_kbps (word audio bitrate, default 48), text_bitrate_kbps (definition and example audio bitrate, default 32), engine (engine name), engine_ver (engine version).

### bitrate_kbps(kind) → number

Returns the corresponding bitrate based on TTS type (Word/Definition/Example). Word audio uses a higher bitrate because dictation quizzes require clear phonemes.

### params_json(kind) → string

Generates the value for the `tts_assets.params_json` column. The field order is fixed (not dependent on map iteration) because this string participates in hashing — the same configuration with a different ordering must not produce two rows/lines.

### input_hash(kind, text) → string

Content address for a single synthesis. Internally calls tts_input_hash, mixing the text, voice, engine, version, and parameters all together.

### desired(kind, text) → DesiredTts

Generates a complete desired synthesis description for an item, containing hash, text, parameters, and bitrate.

## DesiredTts

A desired TTS synthesis for an item, containing: kind, input_hash, text, params_json, bitrate_kbps. Used to diff the desired set against existing assets.

## Constraint

- The field order of params_json cannot be changed — it participates in hashing; changing the order will make the same configuration produce different asset rows/lines.
- engine_ver is intentionally a configured value rather than an adapter-reported value; changing the version number will cause all existing TTS assets to expire.
