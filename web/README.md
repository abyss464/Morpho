# Morpho web

A local web app that teaches the whole released vocabulary in the "4000 Essential English Words" model, all in English: every word comes with a picture, phonetic, audio, part of speech, a one-sentence English definition and an example sentence.

There is one way in. Today shows how many reviews and new words are waiting; Continue opens the **stream**, which mixes reviews and new words into one sequence of steps:

- **New word**: the full card, read aloud (word, definition, example).
- **Explain it**: complete the definition by clicking pieces into its blanks, with decoy pieces from other words; the part that names the word, the prepositions and the punctuation are already in place. Once right after the card, then again a few steps later without the picture.
- **Use it**: pick the word that fills the blank in its example from four look-alike words.
- **Review**: a graduated word comes back on its FSRS schedule, alternating between a rebuild followed by spelling the word back from its definition (letter tiles, one or two letters given) and a fill-in; the rating is derived from how it went and can be changed.

Every step is done with the mouse; arrow and Enter keys are optional shortcuts. The rules (stages, transitions, mixing, ratings) are the shared contract in `docs/contracts/stream.md`, which the Android app implements too.

## Run

From the repository root:

```sh
pnpm --dir web install   # first time only
pnpm --dir web dev       # http://127.0.0.1:30017
```

`pnpm --dir web start` is the same as `dev`. `pnpm --dir web build` type-checks and builds to `web/dist/`; `pnpm --dir web preview` serves that build on the same address with the same data endpoints.

## Data

The app reads a release bundle produced by `morphod export`, read-only, straight from `data/releases/`:

- Bundle: the lexicographically last `data/releases/export-*/` that contains `release.db`. Set `MORPHO_RELEASE` to a bundle name (`export-20260901T122321560Z`) or a path to use another one.
- The bundle is re-resolved on every API request, so a newly cut release shows up on the next page load without restarting the server.
- `release.db` is opened with Node's built-in `node:sqlite` (Node 22.5+). It reads `words`, `senses`, `examples` (the `display_order = 1` sentence and its matched picture), `distractors` and `meta`.
- Media is served from the bundle itself: `/media/img/{hash}.webp` and `/media/audio/{hash}.ogg`, cached immutably, with byte-range support.

Endpoints, provided by the Vite plugin in `server/release.ts`:

| Path | Returns |
|---|---|
| `/api/index` | release meta and every word in learning order as `[word_id, word]` |
| `/api/unit/:n` | full data for unit `n` (20 consecutive words in `learning_order`) |
| `/api/words?ids=1,2,3` | full data for up to 200 word ids |
| `GET`/`POST /api/sync` | the synced progress document: POST merges the sender's into it and returns the result |

## Progress

Stored in the browser's `localStorage` under `morpho-web-v2`, keyed by `word_id` (stable across releases): each word's stage and next step, FSRS cards for graduated words, today's counters, a per-day step history, the daily new-word setting, the step on screen (so a paused stream resumes on it) and the learner's own notes. Progress from the earlier unit-and-review version (`morpho-web-v1`) carries over: its studied words continue in review.

The page also syncs this progress with the Android app through the server (`docs/contracts/sync.md`): on load and a moment after every change it posts its words' cards, learning stages and notes, and takes the merged copy back. The server keeps that copy in `data/sync/progress.json`. The phone reaches the server at the address entered in its Settings; over USB, `adb reverse tcp:30017 tcp:30017` makes `127.0.0.1:30017` on the phone reach it. `MORPHO_WEB_PORT` and `MORPHO_SYNC_FILE` start a second copy with its own port and sync file, for testing.

## Layout

```
server/release.ts           Vite plugin: release resolution, SQLite reads, JSON API, media serving
src/types.ts                JSON shapes shared by server and client
src/api.ts                  fetch + cache for index, units and words
src/store.ts                localStorage progress
src/sync.ts, src/syncdoc.ts progress sync with the server: client, and the shared document and merge
src/stream.ts               the stream: next step, transitions, derived ratings, FSRS
src/explain.ts              definition blanks, pieces and the rebuild puzzle
src/audio.ts                shared audio player (single files or a word-definition-example run)
src/router.ts               hash routes: #/ (Today), #/stream, #/unit/:n (word list)
src/components/             word card pieces, Rebuild, Spell, Fill, Search
src/pages/                  Today, Stream (steps, review result, done), UnitPage
src/styles.css              shared tokens (light/dark) and layout
```
