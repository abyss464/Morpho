# Morpho web

A local web app that teaches the whole released vocabulary in the "4000 Essential English Words" model, all in English: every word is shown as picture, word, phonetic, audio, part of speech, a one-sentence English definition and one example sentence with the word highlighted. Review shows only the word; the learner writes their own explanation from memory, reveals the card to compare, and rates themselves (Again / Hard / Good / Easy, scheduled with FSRS via `ts-fsrs`).

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
- `release.db` is opened with Node's built-in `node:sqlite` (Node 22.5+; Node 24 here). Only `words`, `senses`, `examples` (the `display_order = 1` sentence and its matched picture) and `meta` are read.
- Media is served from the bundle itself, not copied: `/media/img/{hash}.webp` and `/media/audio/{hash}.ogg`, with `Cache-Control: public, max-age=31536000, immutable` and byte-range support.

Endpoints, provided by the Vite plugin in `server/release.ts`:

| Path | Returns |
|---|---|
| `/api/index` | release meta and every word in learning order as `[word_id, word]` |
| `/api/unit/:n` | full data for unit `n` (20 consecutive words in `learning_order`) |
| `/api/words?ids=1,2,3` | full data for up to 200 word ids |

Units are 20 consecutive words of the release's `learning_order`, targets and auxiliaries together.

## Progress

Stored in the browser's `localStorage` under `morpho-web-v1`, keyed by `word_id` (stable across releases):

- `cards`: an FSRS card per studied word. Finishing a unit in the study view creates cards for its words, due immediately.
- `notes`: the latest explanation the learner wrote for each word, shown under that word's entry and in its study card.

Unit progress is derived from which of a unit's words have cards, so it follows the words if a later release regroups them.

## Layout

```
server/release.ts    Vite plugin: release resolution, SQLite reads, JSON API, media serving
src/types.ts         JSON shapes shared by server and client
src/api.ts           fetch + cache for index, units and words
src/store.ts         localStorage progress, FSRS scheduling
src/audio.ts         single shared audio player (plays only on click)
src/router.ts        hash routes: #/, #/unit/:n, #/unit/:n/study/:i, #/unit/:n/done, #/review
src/components/parts.tsx   word card pieces (picture, head, definition, example, explanation)
src/pages/           Home, UnitPage, StudyView (+ unit done), Review
src/styles.css       tokens (light/dark), card, segmented rating, pill switch, chip deck
```
