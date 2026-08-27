# Orchestrator operational reference

The main file holds norms; this file holds operational detail. docs/OPERATIONS.md is the final authority; this records only the facts the orchestration layer uses repeatedly.

## Release chain (order used in wave-2)

1. Merge all code branches → re-run gates.
2. `docker compose build`, then `up -d --force-recreate` with env (`MORPHO_CLIP_URL=http://host.docker.internal:30013`, `MORPHO_CLIP_MODEL=ViT-B-32/laion2b_s34b_b79k`). Start the CLIP sidecar on the host first (OPERATIONS.md §2.2); `/health` must report `clip/1` and the matching model.
3. Wait for `clip_scores` to fill (count plateaus) → `MORPHO_API=http://127.0.0.1:30012/api python3 ops/unapprove_auto.py image` to release pins → reselection converges (selection_changed events plateau).
4. Distractor repair: `POST /api/distractors/rebind-violations`, `{"dry_run":true}` first to review the plan, then apply; re-scan must leave only unresolvable rows.
5. `MORPHO_API=http://127.0.0.1:30012 python3 ops/bulk_approve.py` → readiness converges → `GET /api/releases/preview` gates clean (exportable > 0, gate_failures empty).
6. `docker compose stop` → host-native `MORPHOD_ADAPTERS_ROOT=<repo>/adapters MORPHO_WORDNET_DIR=<repo>/data/wordnet/dict morphod publish --no-build` (ADAPTERS_ROOT is mandatory until #29 is fixed) → `docker compose up -d`.
7. Verify media sync (zero missing vs manifest; watch for legacy flat-layout files at the content_media root — publish's stale cleanup does not scan there, #29 part two).
8. Update ReleaseDatabaseTest count assertions (agent's job) → full gradle gates + assembleFatApkDebug.
9. APK: `app/app/build/outputs/apk/fatApk/debug/app-fatApk-debug.apk`.

## adb install

- Package id is `dev.morpho.debug` (the `dev.morpho` in CLAUDE.md is the application-id base; uninstall needs the suffixed real name).
- Overlay install corrupts the app: `adb uninstall dev.morpho.debug` before installing; verify a clean install via dumpsys `firstInstallTime == lastUpdateTime`.
- Progress-preserving flow: pull with `run-as dev.morpho.debug cat databases/user.db*` → locally `PRAGMA wal_checkpoint(TRUNCATE)` + `VACUUM` into a single file → uninstall → install → `adb push` + `run-as cp` back before first launch.
- This section simplifies once #37 (proper overlay-install support) lands.

## Docker traps

- `.dockerignore` must include `.claude/` — agent worktrees carry core/target dirs reaching tens of GB, and without the entry the build stalls in context packing.
- One `adapter unavailable adapter="sdxl"` WARN per container start is expected (the image packs only tts/morfessor; SDXL/CLIP run host-side).
- The compose data volume is a bind mount, so image rebuilds cannot hurt working.db; still compare dashboard word counts before and after.

## Known engine defects (filed, pending fix)

- #29 publish: repo_root resolves one level too high (work around with MORPHOD_ADAPTERS_ROOT); stale-media cleanup does not scan the legacy flat layout at the content_media root.
- #31: rejecting a selected image candidate can leave the selection row stranded at `selected_by=human` pointing at the rejected candidate; auto-reselection never takes over — fix manually via select+approve.
- Gallery source filter: codex-generated images ingested through the upload API carry the `manual` tag; the `codex` tag is produced only by the engine's built-in generation source (`MORPHO_CODEX_ENABLED`).

## ops script conventions

- `unapprove_auto.py` wants `MORPHO_API` with the `/api` suffix; `bulk_approve.py` without. Unification is on the todo list.
- CLIP-dependent scripts run under `~/Code/vendor/ComfyUI/.venv/bin/python`.
- Library health quick-check (selected image vs slot-1 sentence semantic score): join clip_scores on the word's slot-1 text_hash; < 0.08 is off-topic, 0.08–0.15 is a weak match.
