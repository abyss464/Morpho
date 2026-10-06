# Progress Sync Contract (web/ and app/)

The web app's server keeps one progress document and does all merging. A client syncs by
sending its own document and applying the merged document it gets back. The web client
syncs on its own; the Android app syncs when the learner asks, against an address they
enter. Merging lives in one place: `web/src/syncdoc.ts`.

## 1. Endpoint

| Request | Body | Response |
|---|---|---|
| `POST /api/sync` | the client's document (§2) | the merged document |
| `GET /api/sync` | — | the stored document (empty when nothing was synced yet) |

The server stores the merged document at `data/sync/progress.json` (written to a temporary
file, then renamed). A body that is not a version-1 document is refused with 400.

## 2. Document

```json
{
  "v": 1,
  "words": { "<word_id>": { "card": Card | null, "stage": Stage | null } },
  "notes": { "<word_id>": { "text": "…", "at": "2026-10-06T12:00:00.000Z" } }
}
```

- **Card**, the FSRS card of a word that has graduated:
  `{ "due", "stability", "difficulty", "elapsedDays", "scheduledDays", "reps", "lapses", "state", "lastReview" }`.
  - `due` and `lastReview` are ISO-8601 UTC strings; `lastReview` may be null.
  - `state` is 0 New, 1 Learning, 2 Review, 3 Relearning.
- **Stage**, a word being learned or relearned in the stream (stream.md §1):
  `{ "stage": "learning" | "relearning", "next": "know" | "explain1" | "explain2" | "use", "immediate", "needClean", "thenUse", "flawed" }`.
  Stream positions (`since`) are per client and are never sent.
- A client sends every word it has a card or a stage for. A word it has never met is absent.
- Today's counters, step history, the daily new-word setting and the step on screen stay
  local and are not synced.

Field mapping:
- **Web:** ts-fsrs cards carry `elapsed_days`, `scheduled_days` and `last_review`, and send them as the fields above. A card from the document is stored with `learning_steps` 0.
- **Android:** `FsrsCard` maps field for field.

## 3. Merge (server)

Per word, between the stored entry S and the incoming entry I. The winner's card and stage
are taken together:

1. **Both have a card:** the one with the later `lastReview` wins. A missing `lastReview` counts as oldest; on a tie S stays.
2. **Only one has a card:** it wins. A word graduated or reviewed anywhere is further along than one still being learned elsewhere.
3. **Neither has a card:** the stage further along wins, by `next` (know < explain1 < explain2 < use). On a tie S stays.

Words on only one side are kept. Notes: the later `at` wins. Sync never deletes anything.

## 4. Applying the merged document (clients)

For each word whose merged entry differs from the local one:
- Set the local card to the merged card. Rule 2 means a merged entry never lacks a card the client has.
- Replace the local stage with the merged stage, or remove the local stage when the merged stage is null. An imported stage gets `since` = the client's current stream position, so its spacing starts now.
- If the step on screen is for a changed word, drop it, so the next step is chosen again. If the last review (the one whose rating can still be changed) is for a changed word, drop it too.

Then take each merged note that is newer than the local one. Report how many words changed.

## 5. Clients

- **Web:** syncs on page load and 2 s after every change. It commits locally only when the merged document changed something, so applying a sync does not trigger another. Sync failures are silent: the page works without the server.
- **Android:** Settings has "Sync with the web app":
  - an address field (saved; default `http://127.0.0.1:30017`) and a **Sync now** button;
  - the result: words updated from the web, words sent, or the error.

  It also syncs once, silently, each time Today opens when an address is saved. Over USB, `adb reverse tcp:30017 tcp:30017` makes the default address reach the computer's web app; any other device needs the web app to listen on an address the phone can reach.
