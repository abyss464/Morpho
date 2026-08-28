# Contract #52 — CLIP scoring as subprocess adapter

## Mission

Convert the CLIP image scorer from an external HTTP sidecar (port 30013, host-side GPU process) to a subprocess adapter following `adapter-protocol.md`. The engine spawns the subprocess on demand like TTS/Morfessor/SDXL — no persistent service to start or lose on reboot.

## Required reading

- `docs/contracts/adapter-protocol.md` — envelope, invocation, error taxonomy
- `core/crates/reconcile/src/sources/proc.rs` — subprocess protocol implementation
- `core/crates/reconcile/src/sources/clip.rs` — current HTTP client (being replaced)
- `core/crates/reconcile/src/exec/images.rs` — `ScoreImageClipExecutor`
- `core/crates/reconcile/src/config.rs` — `SourcesConfig::clip_url`, `ADAPTERS`, `ImagesConfig::clip_model`
- `adapters/clip/src/morpho_clip/scorer.py` — model + scoring logic (reuse as-is)
- `adapters/clip/src/morpho_clip/media.py` — file resolution (reuse as-is)

## Changes

### 1. Python adapter (`adapters/clip/`)

Add a subprocess entry point following `adapter-protocol.md`:

**Op: `clip.score`**
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

- `media_root` passed by the engine so the adapter resolves `{root}/{hash[:2]}/{hash}.webp` itself (same layout as the sidecar's `MediaLibrary`).
- Model loads on first invocation within the process. One process per job — the load cost (~2-4s on CPU) is acceptable for steady-state image ingestion.
- Reuse `scorer.py` (`OpenClipScorer`) and `media.py` (`MediaLibrary`) unchanged.
- The HTTP service (`service.py`, `__main__.py`) stays in the tree but is no longer the primary interface. Keep it working (existing HTTP tests pass); a later cleanup can remove it if unwanted.
- Add a `clip-adapter` entry point (console_scripts or `__main__` dispatch) that reads the envelope from stdin, calls `ScoreService.score()`, writes the result envelope to stdout.
- Tests: add subprocess-mode tests alongside the existing HTTP tests.

### 2. Rust engine (`core/`)

**`sources/proc.rs`** — add typed wrapper:
```rust
pub const CLIP_TIMEOUT: Duration = Duration::from_secs(120);

pub async fn clip_score(config: &AdapterConfig, request: ClipScoreRequest<'_>)
    -> Result<ClipScoreResult, TaskError>
{
    call(config, "clip", "clip.score", request, CLIP_TIMEOUT).await
}
```
Request/result types mirror the JSON above. `ClipScoreResult` reuses the existing `ScoreResponse` shape (or maps to it).

**`sources/clip.rs`** — gut the HTTP client. The module either becomes a thin re-export of the proc types, or is removed entirely with its contents moved into `proc.rs`. The `MAX_IMAGES_PER_REQUEST` constant and the request/response types stay (they are referenced by the rule and executor).

**`config.rs`**:
- Remove `clip_url: Option<String>` from `SourcesConfig` and `CLIP_URL_ENV`.
- Add `("clip", "score_image_clip jobs — image selection has no semantic term and ranks on quality alone")` to `ADAPTERS`.
- CLIP availability is now determined by adapter probe (launcher + project dir), same as TTS/Morfessor/SDXL.
- `ImagesConfig::clip_model` and `MORPHO_CLIP_MODEL` stay — the executor still checks model identity.

**`exec/images.rs`** (`ScoreImageClipExecutor::run`):
- Replace `clip::score(&self.context.sources.http, base_url, ...)` with `proc::clip_score(&self.context.sources.adapters, ...)`.
- Remove the `clip_url().is_some()` guard; the dispatcher already skips jobs for unavailable adapters.
- The model identity check (`response.model_ver() != expected`) stays.
- Pass `self.context.media.root()` (or equivalent) as `media_root` in the request.

**`rules/images.rs`** — the CLIP rule currently checks `clip_url().is_some()` to decide whether to derive `ScoreImageClip` jobs. Change to check adapter availability (the CLIP entry in the adapter probe results, or a config flag derived from it).

### 3. Docker (`Dockerfile`)

Add CLIP to the py-builder stage. Install CPU-only torch to keep the image light:

```dockerfile
# CLIP adapter (CPU-only torch — GPU is used when available at runtime)
COPY adapters/clip/ ./clip/
RUN cd clip && \
    uv pip install torch --index-url https://download.pytorch.org/whl/cpu && \
    uv sync --frozen
```

Copy into runtime stage alongside the other adapters:
```dockerfile
COPY --from=py-builder /build/adapters/clip/ /app/adapters/clip/
```

The existing `OpenClipScorer._build()` auto-detects the device (`cuda` if available, else `cpu`). In Docker this means CPU; running morphod natively on a GPU host means GPU — no configuration needed.

### 4. Documentation

- `adapter-protocol.md`: remove the "Not an adapter" section (lines 87-89), add `clip.score` op documentation.
- `clip-service.md`: add a note that the HTTP sidecar is superseded by the subprocess adapter; keep the file for reference.
- `OPERATIONS.md`: remove §2.2 (CLIP sidecar startup), remove `MORPHO_CLIP_URL` from the env table, update §4 ship sequence (no sidecar step).
- `reference.md`: update the release chain (no sidecar start step).

## Boundaries

- **No batching optimization.** Per-word subprocess invocation is sufficient for steady-state. The ~2.5 hour backfill for 3000+ existing unscored words is acceptable as a one-time background task.
- **No GPU passthrough for Docker.** CPU inference for ViT-B-32 is fast enough. GPU acceleration works automatically when running natively.
- **No changes to `clip_scores` table or scoring semantics.** Same model, same normalization, same cosine values.
- **No changes to image selection logic.** The scorer, the "all-or-nothing" rule (§7.12), the hysteresis margin, the distractor veto — untouched.

## Acceptance criteria

1. `MORPHO_CLIP_URL` env var is gone; engine starts and scores images with no sidecar process.
2. `adapters/clip` follows `adapter-protocol.md`: stdin JSON → stdout JSON, exit 0.
3. CLIP availability checked at startup via adapter probe, logged like TTS/Morfessor.
4. Image candidates ingested → next reconciler sweep derives `score_image_clip` → subprocess scores them → `clip_scores` rows written. No manual intervention.
5. Existing `clip_scores` data untouched, new scores numerically consistent.
6. All gates pass: `cargo fmt/clippy/test`, `uv run --directory adapters/clip pytest`, `docker compose build`.
7. Commit on own branch, logical units, explicit paths. No merge, no push.
