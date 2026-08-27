# Morpho — Operations Handbook

Operational runbook for the content pipeline. Read this + `README.md` (the
design whitepaper) + `docs/contracts/` (the interface contracts) to pick the
project up cold. This file holds what the code cannot tell you: current state,
how to run things, and the traps.

Last updated: 2026-08-27, wave-9 CLIP-in-engine + codex source (last cut: release 1.5, 2026.08.27+d034466d).

---

## 1. Where things are

| Path | What |
|---|---|
| `core/` | Rust `morphod` — reconciler + admin API + exporter (one binary) |
| `admin-ui/` | React/AntD admin console (dev: MSW-mocked; real: proxies `/api`) |
| `adapters/` | Python: tts (edge-tts→Opus), morfessor, sdxl (ComfyUI client), codex (image generator) as subprocess CLIs; **clip** as a host-side HTTP sidecar |
| `app/` | Android app (Kotlin/Compose); `app/content_media/` holds the media pack (git-ignored) |
| `data/working.db` | THE production content database (SQLite WAL) — never in `core/data/`, see trap #1 |
| `data/media/{hash[:2]}/{hash}.{webp,ogg}` | content-addressed media store |
| `data/releases/export-*/` | exported release bundles (release.db + img/ + audio/ + manifest.json) |
| `data/wordnet/dict/` | WNdb data files (lemmatizer exceptions + glosses + semantic grouping) |
| `content/wordlists/` | `base-primary-junior.txt` (base) + `target-npee.jsonl` (exam targets) |
| `content/corpora/opensubs-en.txt.gz` | OpenSubtitles EN, 3.7 GB — sentence mining source |
| `ops/` | reusable operator scripts (persisted from the ephemeral scratchpad) |
| `~/Code/vendor/ComfyUI/` | ComfyUI + `.venv` (ROCm torch, open_clip) for CLIP/SDXL |

The working DB is currently at schema `user_version` 7 (wave-9 `clip_scores` + the `codex` image source).

## 2. Starting the engine

**TRAP #1 — the cwd trap.** `morphod` resolves `data/` relative to its working
directory. If you start it from `core/`, it creates a new empty
`core/data/working.db` and silently serves nothing — exports come out with 0
words. Always launch from the repo root, and put the `cd` inside the command so
a backgrounded shell's drifted cwd cannot affect it:

```bash
cd /home/abysser/Code/learning/Morpho && \
PIXABAY_API_KEY='<key>' \
MORPHO_WORDNET_DIR=/home/abysser/Code/learning/Morpho/data/wordnet/dict \
RUST_LOG=warn \
/home/abysser/Code/learning/Morpho/core/target/release/morphod serve
```

Use absolute paths for the binary and env values. If a stray `core/data/`
appears, kill the engine, `rm -rf core/data`, restart correctly.
Health check: `curl -sm3 http://127.0.0.1:8787/api/dashboard`.

The engine is a long-running reconciler; it runs indefinitely by design. A
background-task entry that "ran for hours" with no completion is the engine (or
a stale registry entry after a session ended), not a hang.

Secrets go in env only, never in a committed file. The Pixabay key lives in the
owner's env; an image source without a key is disabled and the chain falls
through to the keyless sources + generation.

### 2.1 Environment variables

Everything below is optional; every default reproduces the behaviour from before
that variable existed. The wave-9 additions are the last six.

| Variable | Default | Effect |
|---|---|---|
| `MORPHOD_CONFIG` / `MORPHOD_DATA_DIR` / `MORPHOD_RELEASES_DIR` / `MORPHOD_BIND` / `MORPHOD_ADMIN_UI_DIST` / `MORPHOD_ADAPTERS_ROOT` | from `morphod.toml` | Paths and bind address. |
| `MORPHO_WORDNET_DIR` | unset | WNdb directory. Unset disables the WordNet definition fallback and semantic grouping. |
| `MORPHO_CORPUS_PATH` | unset | Exam-corpus JSONL. Unset disables that one example source. |
| `UNSPLASH_ACCESS_KEY` / `PEXELS_API_KEY` / `PIXABAY_API_KEY` | unset | Stock providers. Unset = that provider is disabled, never queried. |
| `COMFYUI_URL` | unset | SDXL generation. Unset = no local generation; words honestly report `missing_image`. |
| `MORPHO_SCENE_MODE` | `0` | Prompt SDXL with the word's own slot-1 sentence rather than the bare concept. |
| **`MORPHO_CLIP_URL`** | unset | The CLIP sidecar. Unset means image selection has no semantic term and ranks on the quality prior alone. Set it to `http://host.docker.internal:30013` from the container, `http://127.0.0.1:30013` natively. This switch turns semantic image selection on. |
| **`MORPHO_CLIP_MODEL`** | `ViT-B-32/laion2b_s34b_b79k` | The model the sidecar is expected to be serving. It is stored in `clip_scores.model_ver`, and the executor refuses a sidecar reporting anything else — a silent model swap would file two models' cosines in one column. Changing it does not invalidate the old rows; it stops reading them. |
| **`MORPHO_CODEX_ENABLED`** | `0` | The codex generation source. Doubly gated: it also needs `adapters/codex` on disk and `MORPHO_CODEX_BIN` to resolve. |
| **`MORPHO_CODEX_BIN`** | `codex` | The generator binary. The adapter reads the same variable, so a machine where it does not exist has the source disabled rather than dead-lettering every word. |
| **`MORPHO_CODEX_THRESHOLD`** | `0.22` | Raw CLIP cosine below which a word's best picture is worth replacing. A good match lands near 0.28; the bottom decile falls under 0.20. |
| **`MORPHO_CODEX_BATCH`** | `8` | Most codex jobs one derivation may ask for. The queue is derived, so the rest come back next pass. |
| `MORPHO_CODEX_PROMPT_VER` | `codex/1` | Prompt template version. Bumping it moves the job subject, which is how to request the whole pass again without clearing a mark. |
| `MORPHO_CODEX_ARGS` / `MORPHO_CODEX_TIMEOUT_S` / `MORPHO_CODEX_BLANK_STDDEV` / `MORPHO_CODEX_WEBP_QUALITY` / `MORPHO_CODEX_MODEL` | see `adapters/codex/README.md` | Adapter-side only; morphod does not read them. |
| `MORPHO_CLIP_MEDIA_ROOT` | `data/media` | Sidecar-side only: where *it* sees the media library, which is not where the container sees it. |

### 2.2 Starting the CLIP sidecar

It is a separate process because it needs the GPU, and it is *not* started by
`docker compose` — it runs on the host, under the venv that holds the ROCm build
of torch:

```bash
PYTHONPATH=/home/abysser/Code/learning/Morpho/adapters/clip/src \
/home/abysser/Code/vendor/ComfyUI/.venv/bin/python -m morpho_clip \
  --media-root /home/abysser/Code/learning/Morpho/data/media \
  --host 0.0.0.0 --port 30013
```

`--host 0.0.0.0` because morphod is containerized and reaches the host from
outside its network namespace; `docker-compose.yml` maps
`host.docker.internal` onto the host gateway so the URL in `MORPHO_CLIP_URL`
resolves. Health check: `curl -sm3 http://127.0.0.1:30013/health` — it must
report `"algo_ver":"clip/1"` and the model named in `MORPHO_CLIP_MODEL`, or the
engine will refuse every score it produces.

First request loads the model (a few seconds, plus a weight download on the very
first run). It is idle otherwise: the whole lexicon is a few minutes of work,
then nothing until a candidate arrives.

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
  words. Safe to re-run; approves only unapproved rows. Base URL comes from
  `MORPHO_API` (default `http://127.0.0.1:8787`) — set it when the engine runs in
  Docker: `MORPHO_API=http://127.0.0.1:30012 python3 ops/bulk_approve.py`.
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
- **Re-match images to sentences (CLIP): the engine does this now.** `ops/clip_rematch.py`
  is superseded and kept for reference. Approval implies a pin and a pinned slot
  is untouchable, so a script that re-selects through the admin API hits a frozen
  library (trap §7.9). The engine reads the same cosines natively, at ranking
  time, where a pin is the only thing that can stop it and un-approving releases
  it.

  **Deploying it (the wave-9 ship sequence).** This re-selects over the whole
  library, so budget a convergence wait and a re-approval:

  1. Start the sidecar (§2.2) and verify `/health` reports the model named in
     `MORPHO_CLIP_MODEL`. A mismatch fails every job permanently — loudly, on
     purpose.
  2. Set `MORPHO_CLIP_URL` and restart the engine. Nothing moves yet: the first
     sweeps only *derive* `score_image_clip` jobs, and a word is ranked on
     quality alone until its whole pool is scored. Watch
     `SELECT COUNT(*) FROM clip_scores` climb; the full lexicon is a few minutes.
  3. **Nothing will re-select until the pins come off.** Every settled slot is
     approved, and approval pins. `python3 ops/unapprove_auto.py image` releases
     the pin approval put on `auto` slots (a human override keeps its own).
  4. Converge. The next sweeps rescore under `scorer/5` and re-select on the
     semantic term. Expect large-scale movement.
  5. `MORPHO_API=http://127.0.0.1:30012 python3 ops/bulk_approve.py`, then
     `GET /api/releases/preview`. `question_images_distinct` should now be
     structurally clean: automatic selection refuses a picture a question mate
     shows.
  6. Only then, if wanted: `MORPHO_CODEX_ENABLED=1`. It is P3 and batched.

  Expected convergence: step 2 is bounded by the sidecar (minutes); step 4 by the
  60 s sweep plus TTS re-synthesis for nothing (images have no TTS), so it is
  fast; step 5 is the expensive one, exactly as trap §7.9 describes.
- **Resolve OOV (out-of-scope words in definitions):** three modes via
  `POST /api/oov/{lemma}/resolve` — `{"mode":"promote"}` (make it a learnable
  auxiliary word), `{"mode":"rewrite","def_cand_id","text"}`, or
  `{"mode":"gloss","zh_gloss":"..."}` (Chinese anchor — terminates the chain
  like a base word; the owner's chosen path for un-learnable referenced words).
- **Waive a rate-limited source (e.g. Free Dictionary 522/429):** flip its
  backoff rows to `waived` in `job_state` and seed `waived` rows for never-fetched
  words, then restart — the WordNet fallback takes over immediately. Free
  Dictionary rate-limits on bulk runs.
- **Retry dead letters:** `POST /api/dead-letters/retry {kind,subject_type,subject_id}`
  — most `synth_tts` dead letters are transient edge-tts timeouts (HTTP 4xx
  "Connection timeout"), which succeed on retry.

## 5. Cutting a release

1. Get the cut clean: `GET /api/releases/preview`. `exportable_count` must be > 0
   and `gate_failures` empty. Common blockers and fixes:
   - `dependency_holdback` cascade → the readability graph has an un-shippable
     word pulling others out. Fix the root (gloss/promote un-learnable referenced
     words until OOV queue is empty; `GET /api/oov?status=open` — note that
     endpoint pages 50 at a time and ignores `offset`, so read `oos_queue` +
     `oos_occurrences` directly for the full picture).
     Before resolving anything by hand, ask whether the offending slot even needs
     a decision — a candidate that is already OOV-clean may be sitting right
     behind the selected one:
     ```sql
     -- selected slots that reference an open OOV lemma, and whether the same
     -- (word, pos) has an out-of-scope-free candidate available instead
     WITH clean AS (SELECT dc.def_cand_id, dc.word_id, dc.pos FROM definition_candidates dc
       WHERE NOT EXISTS (SELECT 1 FROM def_tokens t LEFT JOIN words w ON w.lemma = t.lemma
                          WHERE t.def_cand_id = dc.def_cand_id AND w.word_id IS NULL))
     SELECT ds.word_id, ds.pos,
            EXISTS (SELECT 1 FROM clean k WHERE k.word_id = ds.word_id AND k.pos = ds.pos
                      AND k.def_cand_id <> ds.def_cand_id) AS has_clean_alternative
       FROM definition_selections ds
       JOIN def_tokens t ON t.def_cand_id = ds.def_cand_id
       JOIN oos_queue q ON q.oos_lemma = t.lemma AND q.status = 'open'
      WHERE ds.enabled = 1 GROUP BY ds.word_id, ds.pos;
     ```
     If most rows say yes, the scorer picked badly — fix the scorer (trap §7.8)
     rather than promoting a few hundred words into the lexicon.
   - `question_images_distinct` → two of a question's four images are the same.
     Since wave 9 automatic selection cannot *create* this: a picture one of the
     word's bound distractors currently shows is removed from its pool outright,
     and an incumbent that duplicates a mate is replaced without having to clear
     the hysteresis margin. What can still reach the gate is a **pinned** slot (a
     human's choice outranks the veto) or a word whose entire pool is already
     taken — the reconciler never *empties* a slot, so such a word keeps its
     duplicate and reports it here. The fix for the second case is more
     candidates, which `needs_image_candidates` already requests.
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

1. **cwd trap** (§2) — the most repeated mistake.
2. **Backgrounded `cd` does not persist** to the session shell, and a backgrounded
   compound command can kill a just-started engine — start the engine in its own
   command, verify it is up separately.
3. **Free Dictionary rate-limits** on 6000-word runs (522/429). Waive to WordNet
   rather than waiting it out.
4. **`/tmp` scratchpad is wiped on reboot.** Anything reusable belongs in `ops/`
   or the repo, not the scratchpad.
5. **Reconciler can steal a pinned slot-1** the instant a new example candidate is
   minted (UNIQUE(word_id, ex_cand_id) + auto-select filing it into another slot).
   Mint→select→approve tightly and verify; two engine fixes for this are filed.
6. **CLIP dedup fixes visual identity, not semantic family** — contain/container
   both map to "container" scenes. Distractor semantic clustering is open backlog.
7. Android SDK at `~/Android/Sdk`, headless AVD `morpho_wave1`. Full-media APK
   packaging takes minutes (500 MB zip) — not a hang.
8. **A scorer bump can un-ship the whole library.** Under `scorer/2` readability
   was only `0.40 * (1 - 3 * oos / tokens)`, so one out-of-scope token in a
   ten-token definition cost ~0.12 — about what two sense ranks are worth under
   `SENSE_WEIGHT` 0.25. It therefore preferred a common sense that uses an
   unknown word over a clean rarer one, the OOV queue reopened, and
   `oos_pending` cascaded through the dependency closure until nothing was
   exportable. `scorer/3` resolves it with `out_of_scope_factor()`, which
   multiplies the total (1.0 / 0.25 / 0.10 for zero / one / two-or-more bad
   tokens) exactly like `SELF_REFERENCE_FACTOR`, so an unreadable candidate can
   only win a slot no clean one can fill. Readability stays a component to grade
   density among clean candidates. Before bumping `SCORER_ALGO_VER`, check what
   the new weights do to `SELECT COUNT(*) FROM oos_queue WHERE status='open'` —
   and fix the scorer rather than hand-overriding, because a manual override
   pins the slot and freezes it against every future scorer improvement.
9. **Approval invalidation is by design and it is expensive.** Any selection
   change drops the approval, which drops the word out of `ready` and re-queues
   TTS for the new text. Budget a `bulk_approve.py` re-run plus a convergence
   wait after any batch re-selection.
10. **`oos_open` 0 does not mean the export gate is clean.** `sync_oos_queue`
    only inserts a lemma the queue has never seen; a row already sitting at
    `auto_closed` from an earlier cycle is never reopened when the lemma comes
    back into `oos_occurrences`. So a selection change can start referencing an
    unknown word completely silently, and the only place it surfaces is the
    exporter's `definition_token_resolves` gate — which itself only runs over
    words that are already exportable, so it stays invisible while
    `exportable_count` is 0. Read the truth straight from the tables:
    ```sql
    SELECT DISTINCT t.lemma FROM definition_selections ds
      JOIN def_tokens t ON t.def_cand_id = ds.def_cand_id
      LEFT JOIN words w ON w.lemma = t.lemma
     WHERE ds.enabled = 1 AND w.word_id IS NULL ORDER BY t.lemma;
    ```
    Measured 2026-08-27: 182 such lemmas sat behind the 224 the queue reported.
    Reopening `auto_closed` on re-entry is an unfiled engine fix.
11a. **"In lexicon" is not "shippable".** A definition edit can be OOV-clean yet
    still pull an unshippable word into the dependency closure: re-selecting a
    candidate whose tokens include a promoted-but-empty auxiliary (the §7.11
    plurals) flips blockers onto every dependent. The readability gate (lemma
    exists in `words`) and the closure gate (referenced word itself ships) are
    different gates — bulk re-selections must check `ready OR zh_gloss` on
    referenced target/auxiliary lemmas, not bare existence. Observed cases:
    surgery→incisions, decimal→denominator (both replaced with authored
    in-scope definitions).
11. **CLIP scores are not stale, they are absent.** `clip_scores` is keyed on
    `(file_hash, text_hash, model_ver)`, so nothing is ever "out of date" — a
    changed slot-1 sentence, a changed model or a bumped `clip/N` all just look
    up a key that is not there, and the word falls back to quality ranking until
    the sidecar fills it in. Consequences worth knowing: re-selecting a sentence
    a word once had costs nothing (the scores are still there under that text's
    hash); changing `MORPHO_CLIP_MODEL` re-scores the whole library rather than
    correcting it; old rows are never cleaned up — a few dozen bytes each, and
    `media_gc` does not touch them.
12. **The semantic term is per word and all-or-nothing.** A pool where some
    candidates are scored and some are not ranks on quality alone. So a word does
    not move the instant its first score lands — it moves when its *last* one
    does. Mid-backfill, "why has nothing changed" is usually this, and
    `SELECT COUNT(*) FROM image_candidates ic WHERE ic.status='available' AND NOT
    EXISTS (SELECT 1 FROM clip_scores c WHERE c.file_hash = ic.file_hash)` is the
    number to watch.
13. **A promoted OOV lemma can become a permanent blocker.** `{"mode":"promote"}`
    creates an *active* auxiliary that now needs a definition, an example, an
    image and TTS like any other word — and lemmas like `crosspiece`,
    `adposition` or the plural `integers` have no usable candidates anywhere, so
    they sit `missing_example` forever and hold back the whole dependency
    closure. Prefer `{"mode":"gloss"}`; to unstick one already promoted, anchor
    it with `POST /api/words/{id}/gloss`, which needs no queue row.

## 8. Current state & backlog

- **Shipped: release 1.5 (`2026.08.27+d034466d`): 4231 words.** Contents:
  scorer v2→v3 definitions (inflection-aware self-reference gate, 565→~104
  self-referencing selections with the remainder last-resort; sense-commonality
  prior), 482 primary-POS corrections (attorney/add/charm/quality and others now
  carry their common senses), CLIP rematch over the full lexicon with persisted
  scores (`ops/clip_rematch.py` writes clip_scores.json), 129 codex-generated
  images for the worst CLIP scorers (48 wave-1 + 81 wave-2, each above its
  incumbent, selected + approved via `ops/verify_genimg.py`), 464 Chinese gloss
  anchors (up from 229). APK verified: 983 MB fatApkDebug, 4231 words, media
  synced 25904 = manifest. word_id stability preserved — user progress carries
  over from 1.4.
- `morpho-genimg.timer` is disabled and will not be re-enabled. `adapters/codex`
  is the sole generation path, driven by the engine's `gen_image_codex` rule;
  the `ops/` scripts around it (`genimg_cron.sh`, `verify_genimg.py`) remain as
  reference for the prompt and the gates.
- The scorer/2 → scorer/3 incident and its resolution are §7.8; the diagnosis
  SQL for "does this slot need human judgment" is in §5.1.
- **Shipped:** release 1.4 (`2026.08.26+b5ce3f0e`): 4253 words, real dictionary
  definitions, OpenSubtitles example sentences (flagged 62%→0.4%), CLIP-matched
  images, 229 Chinese gloss anchors, per-question image distinctness, bottom-
  anchored quiz layout, prompt auto-play, adaptive mode-2 caption band.
- **Built in wave 9, not yet deployed:** CLIP scoring inside the engine
  (`MORPHO_CLIP_URL`, default off) and the codex image source
  (`MORPHO_CODEX_ENABLED`, default off). Code and docs only — the deploy is §4's
  ship sequence and has not been run against the live database.
- **Open backlog** (see memory `morpho-image-aptness-backlog`):
  - SDXL scene-image generation for words with no apt image in the pool — built
    (`MORPHO_SCENE_MODE=1`, core wave 9, default off), run in an off-peak GPU
    window. Turbo checkpoint at `~/Code/vendor/ComfyUI/models/checkpoints/`.
  - Distractor semantic near-duplication (§7.6).
  - ~30 words have no example (film dialogue lacks the vocabulary); a few
    proper-noun lemmas stay flagged.
  - App release-signing config (release APK is unsigned).
- **Two engine fixes** are filed to separate sessions: reconciler stealing
  pinned slot-1 selections, and `sentence::locate` e-stem over-matching.

## 9. The subsystems' own test/build commands

- core: `cd core && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- adapters: `uv run --directory adapters/<name> pytest` — `common`, `tts`,
  `morfessor`, `sdxl`, `codex`, `clip`. None of them needs a GPU, a model or a
  quota: the clip suite substitutes a stub for the model and the codex suite
  substitutes a stub script for the generator.
- admin-ui: `cd admin-ui && pnpm exec tsc --noEmit && pnpm exec eslint . && pnpm exec vitest run && pnpm build`
- app: see §6.4
