# Morpho — Operations Handbook

Operational runbook for the content pipeline. Read this + `README.md` (the
design whitepaper) + `docs/contracts/` (the interface contracts) to pick the
project up cold. This file holds what the code cannot tell you: current state,
how to run things, and the traps that cost real time.

Last updated: release 1.4 (2026.08.26+b5ce3f0e).

---

## 1. Where things are

| Path | What |
|---|---|
| `core/` | Rust `morphod` — reconciler + admin API + exporter (one binary) |
| `admin-ui/` | React/AntD admin console (dev: MSW-mocked; real: proxies `/api`) |
| `adapters/` | Python CLIs: tts (edge-tts→Opus), morfessor, sdxl (ComfyUI client) |
| `app/` | Android app (Kotlin/Compose); `app/content_media/` holds the media pack (git-ignored) |
| `data/working.db` | THE production content database (SQLite WAL) — never in `core/data/`, see trap #1 |
| `data/media/{hash[:2]}/{hash}.{webp,ogg}` | content-addressed media store |
| `data/releases/export-*/` | exported release bundles (release.db + img/ + audio/ + manifest.json) |
| `data/wordnet/dict/` | WNdb data files (lemmatizer exceptions + glosses + semantic grouping) |
| `content/wordlists/` | `base-primary-junior.txt` (base) + `target-npee.jsonl` (exam targets) |
| `content/corpora/opensubs-en.txt.gz` | OpenSubtitles EN, 3.7 GB — sentence mining source |
| `ops/` | reusable operator scripts (persisted from the ephemeral scratchpad) |
| `~/Code/vendor/ComfyUI/` | ComfyUI + `.venv` (ROCm torch, open_clip) for CLIP/SDXL |

The working DB is currently at schema `user_version` 6 (wave-7 gloss anchors).

## 2. Starting the engine

**TRAP #1 — the cwd trap (has bitten many times).** `morphod` resolves
`data/` relative to its working directory. If you start it from `core/`, it
creates a BRAND-NEW EMPTY `core/data/working.db` and silently serves nothing —
exports come out with 0 words. ALWAYS launch from the repo root, and put the
`cd` INSIDE the command so a backgrounded shell's drifted cwd can't betray you:

```bash
cd /home/abysser/Code/learning/Morpho && \
PIXABAY_API_KEY='<key>' \
MORPHO_WORDNET_DIR=/home/abysser/Code/learning/Morpho/data/wordnet/dict \
RUST_LOG=warn \
/home/abysser/Code/learning/Morpho/core/target/release/morphod serve
```

Use ABSOLUTE paths for the binary and env values. If you ever see a stray
`core/data/` appear, kill the engine, `rm -rf core/data`, restart correctly.
Health check: `curl -sm3 http://127.0.0.1:8787/api/dashboard`.

The engine is a long-running reconciler — it is SUPPOSED to run forever. A
background-task entry that "ran for hours" with no completion is just the
engine (or a stale registry entry after a session ended); it is not a hang.

Secrets go in env only, never in a committed file. The Pixabay key lives in the
owner's head / this session's env; image sources without a key are simply
disabled (the chain falls through to the keyless sources + generation).

## 3. The reconciler model (how content gets made)

Level-triggered: desired state = every active non-anchored word has all assets
ready. The engine continuously fetches/generates whatever is missing. You never
"run a stage"; you change state (approve, select, gloss, reject) via the admin
API and the reconciler catches up on its next sweep (~60 s full pass, or faster
on change events). Everything is idempotent; staleness is a hash comparison.

Editorial acts are HTTP POSTs with header `X-Morpho-User: <name>`. All the
operator scripts in `ops/` are just batched versions of these.

## 4. Common operations

All scripts assume the engine is up at `127.0.0.1:8787`. Python scripts that use
CLIP need the ComfyUI venv: `~/Code/vendor/ComfyUI/.venv/bin/python`.

- **Bulk-approve everything ready:** `python3 ops/bulk_approve.py` — approves every
  enabled definition selection, example slot, and image selection for active
  words. Safe to re-run; approves only unapproved rows.
- **Let a scorer bump actually take effect:** `python3 ops/unapprove_auto.py
  [definition|example|image ...]`. Approval implies a pin and a pinned slot is
  untouchable by auto-selection, so after a `bulk_approve.py` run the library is
  frozen: bumping `SCORER_ALGO_VER` rescores everything and moves nothing.
  Un-approving an `auto` slot releases the pin approval put there (a human
  override keeps its own), the next sweep re-selects, then re-run
  `bulk_approve.py`. Defaults to definitions.
- **Upgrade example sentences from subtitles:** `ops/mine_subs.py` streams the
  corpus and writes per-lemma top-K sentences to a JSON; a follow-up step mints
  them as `manual` example candidates and selects slot 1 (see release 1.4 notes).
  Highlight offsets are UTF-8 BYTE offsets into the CANONICALIZED text.
- **Re-match images to sentences (CLIP):** `~/Code/vendor/ComfyUI/.venv/bin/python
  ops/clip_rematch.py` — for each word, embeds its image candidates + its slot-1
  sentence and re-selects the best image-text match, with per-question visual
  de-duplication. ~5 min on the 7900 XTX for the full lexicon.
- **Resolve OOV (out-of-scope words in definitions):** three modes via
  `POST /api/oov/{lemma}/resolve` — `{"mode":"promote"}` (make it a learnable
  auxiliary word), `{"mode":"rewrite","def_cand_id","text"}`, or
  `{"mode":"gloss","zh_gloss":"..."}` (Chinese anchor — terminates the chain
  like a base word; the owner's chosen path for un-learnable referenced words).
- **Waive a rate-limited source (e.g. Free Dictionary 522/429 storms):** flip its
  backoff rows to `waived` in `job_state` and seed `waived` rows for never-fetched
  words, then restart — the WordNet fallback takes over instantly. (Free
  Dictionary rate-limits hard on bulk runs; this happened twice.)
- **Retry dead letters:** `POST /api/dead-letters/retry {kind,subject_type,subject_id}`
  — most `synth_tts` dead letters are transient edge-tts timeouts (HTTP 4xx
  "Connection timeout"), which succeed on retry.

## 5. Cutting a release

1. Get the cut clean: `GET /api/releases/preview`. `exportable_count` must be > 0
   and `gate_failures` empty. Common blockers and fixes:
   - `dependency_holdback` cascade → the readability graph has an un-shippable
     word pulling others out. Fix the root (gloss/promote un-learnable referenced
     words until OOV queue is empty; `GET /api/oov?status=open`).
   - `question_images_distinct` → two of a question's four images are the same
     (or CLIP-identical family). Re-select a distinct candidate for one side.
   - `*_not_approved` → run `ops/bulk_approve.py`.
2. `POST /api/releases/export {"notes":"..."}` → writes `data/releases/export-<ts>/`
   with `release.db`, `img/`, `audio/`, `manifest.json`. Version is
   `YYYY.MM.DD+<hash8>`, byte-deterministic for identical content.
3. Refresh the app (see §6).

An empty cut is a valid outcome (dependency closure can legitimately empty) —
the exporter never blocks it; the UI warns.

## 6. Refreshing the app with a new release

1. Copy `release.db` → `app/app/src/main/assets/release.db`.
2. rsync `img/`+`audio/` into `app/content_media/src/main/assets/content_media/`,
   then sync EXACTLY to `manifest.json` — add missing, and trash
   no-longer-referenced files with `gio trash` (never `rm` the owner's files).
   Verify zero missing / zero extra against the manifest.
3. Update `app/app/src/test/kotlin/dev/morpho/data/db/ReleaseDatabaseTest.kt`:
   `content_version` and any changed count assertions (read actual counts from the
   new release.db). This test pins the export by design — it fails on a new
   export until you update it deliberately.
4. Build + test:
   ```bash
   cd app && ./gradlew :domain:test :app:testFatApkDebugUnitTest \
     :app:assembleFatApkDebug :app:assembleFatApkRelease
   ```
5. Deliverable APK: `app/app/build/outputs/apk/fatApk/debug/app-fatApk-debug.apk`
   (~500 MB, media embedded — install THIS, not padDebug which ships no media).
   Release variant is unsigned (no signing config yet).

Media and APKs are git-ignored; `release.db` stays tracked (~5 MB, the unit tests
read it). `word_id` is stable across releases, so user progress survives updates.

## 7. Traps and hard-won facts

1. **cwd trap** (§2) — the single most repeated mistake.
2. **Backgrounded `cd` doesn't persist** to the session shell, and a backgrounded
   compound command can kill your own just-started engine — start the engine in
   its own command, verify it's up separately.
3. **Free Dictionary rate-limits hard** on 6000-word runs (522/429). Waive to
   WordNet rather than waiting it out.
4. **`/tmp` scratchpad is wiped on reboot.** Anything reusable belongs in `ops/`
   or the repo, not the scratchpad. (This handbook exists because of that.)
5. **Reconciler can steal a pinned slot-1** the instant a new example candidate is
   minted (UNIQUE(word_id, ex_cand_id) + auto-select filing it into another slot).
   Mint→select→approve tightly and verify; two engine fixes for this were filed.
6. **CLIP dedup fixes visual identity, not semantic family** — contain/container
   both map to "container" scenes. Distractor semantic clustering is open backlog.
7. Android SDK at `~/Android/Sdk`, headless AVD `morpho_wave1`. Full-media APK
   packaging takes minutes (500 MB zip) — not a hang.

## 8. Current state & backlog

- **Shipped:** release 1.4 (`2026.08.26+b5ce3f0e`): 4253 words, real dictionary
  definitions, OpenSubtitles example sentences (flagged 62%→0.4%), CLIP-matched
  images, 229 Chinese gloss anchors, per-question image distinctness, bottom-
  anchored quiz layout, prompt auto-play, adaptive mode-2 caption band.
- **Open backlog** (see memory `morpho-image-aptness-backlog`):
  - SDXL scene-image generation for words with no apt image in the pool — built
    (`MORPHO_SCENE_MODE=1`, core wave 9, default OFF), run in an off-peak GPU
    window. Turbo checkpoint at `~/Code/vendor/ComfyUI/models/checkpoints/`.
  - Distractor semantic near-duplication (§7.6).
  - ~30 words have no example (film dialogue lacks the vocabulary); a handful of
    proper-noun lemmas stay flagged.
  - App release-signing config (release APK is unsigned).
- **Two engine fixes** were spun off to separate sessions: reconciler stealing
  pinned slot-1 selections, and `sentence::locate` e-stem over-matching.

## 9. The subsystems' own test/build commands

- core: `cd core && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- adapters: `uv run --directory adapters/<name> pytest`
- admin-ui: `cd admin-ui && pnpm exec tsc --noEmit && pnpm exec eslint . && pnpm exec vitest run && pnpm build`
- app: see §6.4
