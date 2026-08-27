# Handoff — wave 9: CLIP in the engine + the codex image source

Branch `worktree-agent-af41b483305ed8ad6`, four commits on top of `0ea2f5d`.
Working tree clean. Backlog #8 (CLIP never reached the engine) and #19 (the
codex pipeline was an external script).

## Status: complete and verified

Every check was run against the committed state, not an earlier one:

| Suite | Result |
|---|---|
| `cargo fmt --check` + `clippy --workspace --all-targets -D warnings` | clean |
| `cargo test --workspace` | **781 passed, 0 failed** |
| `uv run --directory adapters/<n> pytest` | common 48 · tts 98 · morfessor 61 · sdxl 92 · **clip 36** · **codex 34** |
| `pnpm exec tsc --noEmit` / `eslint .` / `vitest run` | clean · clean · 79 passed |

Nothing is half-finished. Nothing is untested. The one thing that has **not**
happened is the deployment itself — by instruction: no API calls to port 30012,
no writes to `data/working.db`, no mass rescore.

## Commits

| | |
|---|---|
| `d09ee24` | `feat(core)` — CLIP scoring in image selection, `clip_scores` schema, codex source rule/executor |
| `f8b633d` | `feat(ops)` — `adapters/clip` sidecar, `adapters/codex` generator, Docker/compose wiring |
| `751fcb9` | `docs` — `clip-service.md`, `codex.generate` op, README architecture, OPERATIONS runbook |
| `7fb355e` | `fix(core)` — incumbent scored on its pool's ruler; admin-ui knows `codex` |

## The two decisions worth re-reading before changing anything

**CLIP is an HTTP sidecar, not a subprocess adapter.** morphod's container has no
accelerator, so spawning `uv run --project adapters/clip` inside it would spawn a
process that cannot load the model. `adapters/sdxl` already answered this — GPU
work outside the engine, reached as a client. The difference is only who wrote
the server: ComfyUI is somebody else's, so `adapters/sdxl` is a thin client for
it; CLIP has none, so `adapters/clip` *is* the server. Content hashes cross the
wire, never bytes. Contract: `docs/contracts/clip-service.md`.

**The cosine is applied at ranking time, never cached in `auto_score`.** It
depends on which sentence slot 1 currently holds, which is mutable state; caching
it under `scorer_ver` would make an unrelated edit invalidate scores across the
lexicon. Same reasoning the codebase already recorded for the duplicate penalty
(`score::image_selection_score`). `clip_scores` is content-addressed on
`(file_hash, text_hash, model_ver)` like `tts_assets` — never stale, only absent.

## Deployment prerequisites

### 1. Start the sidecar (host-side; `docker compose` does not start it)

```bash
PYTHONPATH=/home/abysser/Code/learning/Morpho/adapters/clip/src \
/home/abysser/Code/vendor/ComfyUI/.venv/bin/python -m morpho_clip \
  --media-root /home/abysser/Code/learning/Morpho/data/media \
  --host 0.0.0.0 --port 30013
```

`--host 0.0.0.0` because morphod is containerized. Verify before going further:

```bash
curl -sm3 http://127.0.0.1:30013/health
```

It must report `"algo_ver":"clip/1"` and the model named in `MORPHO_CLIP_MODEL`.
A mismatch fails every job permanently and loudly — that is the point of the
identity check, not a bug to work around.

### 2. Environment variables

Full table in `docs/OPERATIONS.md` §2.1. The ones that matter:

| Variable | Default | Effect |
|---|---|---|
| `MORPHO_CLIP_URL` | unset | **The switch.** Unset = image selection has no semantic term and ranks on the quality prior alone, bit for bit as before. Use `http://host.docker.internal:30013` (compose maps it to the host gateway). |
| `MORPHO_CLIP_MODEL` | `ViT-B-32/laion2b_s34b_b79k` | Stored in `clip_scores.model_ver`. Changing it re-scores the library rather than correcting it. |
| `MORPHO_CODEX_ENABLED` | `0` | The codex source. Also needs `adapters/codex` on disk and `MORPHO_CODEX_BIN` resolving — either missing disables it cleanly. |
| `MORPHO_CODEX_THRESHOLD` | `0.22` | Cosine below which a word's best picture is worth replacing. |
| `MORPHO_CODEX_BATCH` | `8` | Codex jobs per derivation. |

### 3. Rescore sequence (OPERATIONS §4, never run against live data)

1. Sidecar up, `/health` verified.
2. Set `MORPHO_CLIP_URL`, restart the engine. **Nothing moves yet** — the term is
   per word and all-or-nothing, so a word waits for its *last* score, not its
   first. Watch `SELECT COUNT(*) FROM clip_scores` fill; minutes for the lexicon.
3. `python3 ops/unapprove_auto.py image` — **this is the step whose absence made
   `ops/clip_rematch.py` a no-op twice.** Approval implies a pin and a pinned slot
   is untouchable (trap §7.9). A human override keeps its own pin.
4. Converge. Expect real movement: this is the first time anything in the engine
   has known what a picture depicts.
5. `MORPHO_API=http://127.0.0.1:30012 python3 ops/bulk_approve.py`, then
   `GET /api/releases/preview`. `question_images_distinct` should be structurally
   clean — automatic selection now refuses a picture a question mate shows.
6. Only then, optionally, `MORPHO_CODEX_ENABLED=1`. P3 and batched, so it
   trickles.

Convergence: step 2 is bounded by the sidecar; step 4 by the 60 s sweep (images
carry no TTS, so nothing re-synthesizes); step 5 is the expensive one, exactly as
trap §7.9 describes.

## Migration

`user_version` 6→7, additive and reversible: new `clip_scores` table, and
`image_candidates.source` widened for `codex` (a rebuild, because SQLite cannot
alter a `CHECK`). `Migration` gained a `creates` field with its exact inverse, as
its own doc comment invited. The rung is exercised by
`a_wave_six_database_gains_clip_scores_and_the_codex_source`.

`SCORER_ALGO_VER` 4→5 leaves every stored score bit-for-bit identical; it moves so
a deploy restamps every candidate under the build that decides slots
semantically. Definition scoring is untouched, so the §7.8 `oos_queue` check is a
no-op here.

## Owner ruling, as implemented

Codex generation conditions on the slot-1 sentence and nothing else.
`slot1_sentence` is required in the payload type, the adapter parameter and the
prompt; a word without one is **deferred** by the rule, never drawn from its
lemma. The wave-2 prompt wording is carried over verbatim, versioned `codex/1`.

## Known limits, stated rather than hidden

- **Visual near-duplicate dedup is not implemented.** `clip_rematch.py` used
  image↔image cosine ≥ 0.92; that needs embeddings at selection time (~53 MB of
  vectors per pass) or a combinatorial pair table. The per-question veto is on
  **byte identity** — exactly what the exporter's gate checks — plus the existing
  lexicon-wide penalty. The near-identity gap is the one already logged as
  OPERATIONS §7.6.
- **A cornered word keeps its duplicate.** The reconciler never empties a slot, so
  a word whose whole pool is spoken for reports the collision at the export gate
  rather than losing its picture. Asserted non-thrashing.
- **Two mates could oscillate** if their *alternatives* are also the same hash —
  the same characteristic the existing duplicate penalty has.
- `clip_scores` is fully scanned twice per pass (~26k rows, ~10 ms). In line with
  the existing fact-loading design; would want narrowing past ~100k rows.
- `docker build` was not run. The Dockerfile change is mechanical: `adapters/codex`
  is copied in, its generator binary deliberately is not.

## Resume pointer

Nothing to resume in code. The next action is the orchestrator's: verify, merge
into `dev/wave-1`, then run the rescore sequence above against the live engine.
