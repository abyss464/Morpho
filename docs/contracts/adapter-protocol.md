# Adapter Protocol (morphod ⇄ adapters/*)

Adapters are stateless Python CLIs under `adapters/`, managed with `uv` (one pyproject per adapter, shared `adapters/common/` allowed). morphod spawns one process per job (Morfessor: per batch), writes a single JSON request to stdin, reads a single JSON response from stdout, kills on timeout. Logs go to stderr only — stdout is reserved for the response JSON. Exit code 0 whenever the protocol was honored (including error responses); non-zero only on protocol-level crashes.

## Envelope

Request: `{"op": "<name>", "params": {...}}`
Response: `{"ok": true, "result": {...}}` or
`{"ok": false, "error": {"kind": "permanent"|"transient"|"rate_limited", "message": "...", "retry_after_ms": 0}}`

Error taxonomy drives morphod's behavior: `permanent` → record completion/failure, never retry; `transient` → exponential backoff; `rate_limited` → park the lane until `retry_after_ms`.

Output files: morphod passes `out_path` (a temp path it owns). The adapter writes the file, morphod hashes and renames it into the content-addressed store. Adapters never touch `data/` themselves. Idempotency: same request → same output bytes wherever the engine allows (pin versions, fixed seeds, deterministic encoder settings).

## Ops

### `tts.synthesize` (adapters/tts, edge-tts)

```json
{"op": "tts.synthesize", "params": {
  "text": "well meaning and kindly",
  "voice": "en-US-AriaNeural",
  "rate": "+0%", "pitch": "+0Hz",
  "format": "ogg_opus", "bitrate_kbps": 32,
  "out_path": "/tmp/.../x.ogg"
}}
→ {"ok": true, "result": {"duration_ms": 2140, "engine_ver": "edge-tts/7.0.0"}}
```

edge-tts outputs mp3; the adapter transcodes to mono Opus at the requested bitrate via ffmpeg (pinned settings) before writing `out_path`.

### `morfessor.segment` (adapters/morfessor, batch)

```json
{"op": "morfessor.segment", "params": {"words": ["benevolent", "unhappiness"]}}
→ {"ok": true, "result": {"segments": {"benevolent": ["bene", "volent"],
                                        "unhappiness": ["un", "happi", "ness"]},
                           "model_ver": "morfessor/2.0.6+model-2026-08"}}
```

### `sdxl.generate` (adapters/sdxl, local ComfyUI client)

```json
{"op": "sdxl.generate", "params": {
  "prompt": "a scene depicting the concept of 'abandon': ...",
  "negative_prompt": "text, watermark, logo",
  "seed": 42, "width": 768, "height": 576,
  "out_path": "/tmp/.../x.webp"
}}
→ {"ok": true, "result": {"model": "sdxl-base-1.0", "seed": 42}}
```

Unconfigured backend (no ComfyUI reachable) → `{"ok": false, "error": {"kind": "permanent", "message": "sdxl backend not configured"}}` so the word surfaces in dead letters instead of retry-looping.

## Timeouts (enforced by morphod)

tts 60 s · morfessor 120 s/batch · sdxl 600 s

## Wave-2 normative rulings (conductor, 2026-08-26)

1. **Invocation**: morphod spawns `uv run --project adapters/<name> <name>-adapter` from the repo root. morphod owns the `out_path` staging directory (creates before spawn, cleans after result handling); adapters never write anywhere else.
2. **`tts.synthesize` gains optional `volume`** (edge-tts syntax, default `"+0%"`). It is part of `params_json` and therefore feeds the TTS `input_hash`.
3. **Unknown op** → `{"ok": false, "error": {"kind": "permanent", ...}}` with exit 0 (the envelope was honored). Exit code **2** = protocol crash (malformed stdin, handler escape); morphod maps any non-zero exit to `Transient`.
4. **Morfessor batching**: until a corpus-pretrained model ships in `adapters/morfessor/model/`, morphod batches ≥300 distinct words per `morfessor.segment` call (the ad-hoc trainer refuses <100 and degrades below ~300). With a pretrained model present, any batch size is fine.
