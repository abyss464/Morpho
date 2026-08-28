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

### `codex.generate` (adapters/codex, external image generator)

```json
{"op": "codex.generate", "params": {
  "word_id": 4821,
  "lemma": "abandon",
  "pos": "verb",
  "primary_definition": "to give up completely",
  "slot1_sentence": "She had to abandon the car in the flood.",
  "prompt_ver": "codex/1",
  "width": 768, "height": 576,
  "out_path": "/tmp/.../image.webp"
}}
→ {"ok": true, "result": {"model": "codex", "prompt": "Generate one …"}}
```

The last link in the image chain: every stock library, both keyless second passes and local SDXL come first, and a word only reaches here when CLIP scores its best picture below threshold against its own sentence.

**`slot1_sentence` is required** (owner ruling, wave 9). What this source draws is the scene that sentence describes; `lemma`, `pos` and `primary_definition` are disambiguation — which sense the sentence is using — never the subject. Mode 1 asks the learner to match sentence to picture, and the CLIP score that decides whether the result wins its slot queries with that same sentence, so conditioning on anything else would score the picture against a different target than the one it must satisfy. A word with no slot-1 sentence is **deferred by the engine**, not generated from its lemma: `gen_image_codex` derives nothing for it until an example lands.

`prompt_ver` names the template the adapter must draw under; a version it does not implement is a permanent failure rather than a silent substitution, because the candidate's `source_ref` records which template drew it. `result.prompt` is what was actually sent, stored on the candidate as `query_used`.

Two gates live here and nowhere else, because both can only be answered next to the file: exit-zero-with-nothing-written (an ordinary content-policy refusal → permanent) and a blank canvas (grayscale stddev below `MORPHO_CODEX_BLANK_STDDEV`, ported from `ops/verify_genimg.py` → permanent). Everything else — is this apt, is it better than the incumbent, is it somebody else's — the engine decides, because it has CLIP and a database.

Unconfigured backend (`MORPHO_CODEX_BIN` resolves to nothing) → `{"ok": false, "error": {"kind": "permanent", "message": "codex backend not configured"}}`. In practice morphod checks the same variable before deriving, so this is a race guard rather than the normal path: an absent generator *disables* the source.

## Timeouts (enforced by morphod)

tts 60 s · morfessor 120 s/batch · clip 120 s · sdxl 600 s · codex 900 s

The codex adapter's own budget (`MORPHO_CODEX_TIMEOUT_S`, default 840 s) sits below morphod's, so a slow hosted queue is reported as a classifiable timeout rather than being killed mid-write.

### `clip.score` (adapters/clip, image-text similarity)

```json
{"op": "clip.score", "params": {
  "text": "She had to abandon the car in the flood.",
  "images": ["aa11bb22...", "cc33dd44..."],
  "media_root": "/app/data/media"
}}
→ {"ok": true, "result": {
  "model": "ViT-B-32/laion2b_s34b_b79k",
  "algo_ver": "clip/1",
  "scores": [{"file_hash": "aa11bb22...", "similarity": 0.2731}],
  "missing": ["cc33dd44..."]
}}
```

`media_root` is passed by the engine so the adapter resolves `{root}/{hash[:2]}/{hash}.webp` itself. `text` is the word's selected slot-1 sentence, scored verbatim — the adapter must not trim or case-fold it, because morphod files the answer under a hash of the text it sent. `images` is at most 64 content hashes; order is preserved in `scores`. `similarity` is the cosine of two L2-normalized embeddings, in `[-1, 1]` (ViT-B-32 image-text cosines land between about 0.08 and 0.32). `missing` reports hashes the media library could not resolve — reported, never fatal: one file lost must not cost a word the scores of its other candidates. Model loads on first invocation within the process; one process per job, so the ~2-4 s load cost is paid each time — acceptable for steady-state image ingestion. CPU inference in Docker, GPU when running natively (auto-detected).

## Wave-2 normative rulings (conductor, 2026-08-26)

1. **Invocation**: morphod spawns `uv run --project adapters/<name> <name>-adapter` from the repo root. morphod owns the `out_path` staging directory (creates before spawn, cleans after result handling); adapters never write anywhere else.
2. **`tts.synthesize` gains optional `volume`** (edge-tts syntax, default `"+0%"`). It is part of `params_json` and therefore feeds the TTS `input_hash`.
3. **Unknown op** → `{"ok": false, "error": {"kind": "permanent", ...}}` with exit 0 (the envelope was honored). Exit code **2** = protocol crash (malformed stdin, handler escape); morphod maps any non-zero exit to `Transient`.
4. **Morfessor batching**: until a corpus-pretrained model ships in `adapters/morfessor/model/`, morphod batches ≥300 distinct words per `morfessor.segment` call (the ad-hoc trainer refuses <100 and degrades below ~300). With a pretrained model present, any batch size is fine.
