# Admin API Contract (morphod ⇄ admin-ui)

> **Wave-2 normative rulings (conductor, 2026-08-26).** These resolve all wave-1 drift reports and OVERRIDE anything below that conflicts:
>
> 1. **Wire shapes**: `admin-ui/src/api/types.ts` is the normative reference for every request/response body. morphod conforms to it exactly — JSON booleans are real booleans, TEXT-JSON columns (`blockers`, `score_detail`, `stats_json`, `events.detail`) are decoded server-side into objects.
> 2. **Word-scoped mutations return the full `WordDetail`** (one write cascades; the row alone is useless).
> 3. **Actor header is `X-Morpho-User`** (admin-ui migrates off `X-Admin-User`), default actor `local`.
> 4. **Blocker vocabulary** = the `BlockerCode` union in types.ts (incl. `distractor_{1,2,3}_not_ready`, `not_in_plan`). Core emits exactly these codes.
> 5. **`words` gains a reconciler-owned `core_ready` column** (see working-db.sql); `DistractorView.core_ready` reads it, no derivation hacks.
> 6. **TTS status at API level** is `ready | failed | missing` (`missing` = in `tts_desired`, no `tts_assets` row for the current voice/params config).
> 7. **SSE `/stream` frame**: `event: change`, `data: {"entity_type": string, "entity_ids": (number|string)[]}`, coalesced ≤250 ms, `: ping` comment every 30 s.
> 8. **Dashboard `words.auxiliary` counts ACTIVE auxiliaries only.**
> 9. `GET /dead-letters` accepts optional `?page&page_size`; `{items,total}` envelope always.
> 10. **`OovOccurrence.suggested_rewrite`**: newest `available` `llm_rewrite` candidate text for that definition, else `null`.
> 11. Media content types: `image/webp`, `audio/ogg`; `Cache-Control: immutable`.
> 12. A dedicated `/jobs` screen is deferred; the endpoint stays live for the dashboard.
>
> **Wave-3 rulings:**
>
> 13. **`TtsStatusView.status` is `failed`** whenever a dead `job_state` row exists for that TTS input (agreeing with the word's `tts_failed` blocker); `missing` strictly means "not yet attempted or still retrying".
> 14. `GET /dead-letters` accepts optional `?rate_key=` filter. Bulk retry/waive stays client-composed over the per-key endpoints.
> 15. `releases` gains a `word_count` column (migration); `Release.word_count` reads it, not the audit log.
> 16. An empty release passing all gates is VALID (dependency closure may legitimately empty the cut); UI warns, never blocks.
> 17. Adapter invocation must be cwd-independent: morphod resolves the adapters directory from config (`adapters_root`, default = repo root relative to the config file), and `morphod status` + startup logs report each adapter's availability.
> 18a. **Gloss anchors (wave 7, owner ruling)**: a word referenced by definitions but not learnable may carry a short Chinese gloss (`words.zh_gloss`) that terminates the readability chain like a base word — readiness treats dependencies on glossed words as satisfied, the export closure drops edges into them, and they ship as `release.db/gloss_anchors`. New endpoint `POST /words/{id}/gloss {zh_gloss}` (and DELETE to clear); OOV resolve gains `{mode: "gloss", zh_gloss}`. A glossed word needs no other assets; if auxiliary and otherwise unreferenced it still counts as live while glossed+referenced.
> 18. **Keyless real content sources** (owner's original design listed Wikimedia Commons): image sources gain `wikimedia` and `openverse` (no credentials, license metadata recorded per candidate); example sources gain `freedict` (mined from the same Free Dictionary payload as definitions) and `tatoeba`. Priority: keyed stock sources when configured, else wikimedia → openverse → sdxl fallback; examples manual > exam_corpus > freedict > tatoeba > llm. admin-ui's `ImageSource`/`ExampleSource` unions in types.ts must gain the new values (admin-side task).

Base path `/api`. JSON everywhere. Errors: `{"error": {"code": "string", "message": "string"}}` with proper HTTP status. Pagination: `?page=1&page_size=50` → `{"items": [...], "total": n}`. All mutations write an `events` row and return the updated resource. IDs are integers unless noted.

admin-ui develops against MSW mocks implementing exactly these shapes; morphod implements them verbatim. Divergence is a contract change and goes through the conductor.

## Dashboard & observability

| Method | Path | Purpose |
|---|---|---|
| GET | `/dashboard` | `{words: {total, target, auxiliary, ready, blocked}, assets: {definitions, examples, images, tts: {ready, missing, failed}}, oos_open, dead_letters, plan: {plan_id, built_at, group_count}, recent_events: Event[20]}` |
| GET | `/events?entity_type=&entity_id=&page=` | Audit log, newest first |
| GET | `/jobs` | Live queue snapshot from memory: `{in_flight: JobView[], backoff: JobView[], lanes: {rate_key: {queued, running, limit}}}` |
| GET | `/stream` | SSE of `ChangeEvent {entity_type, entity_ids[]}` for live UI refresh |

## Words

| Method | Path | Purpose |
|---|---|---|
| GET | `/words?role=&ready=&blocker=&group=&q=&page=` | List with rollup: `{word_id, lemma, role, ready, blockers[], has_image, sense_count, example_count, tts_missing}` |
| GET | `/words/{id}` | Full detail: word fields + per-slot candidates & selections (definitions grouped by pos, examples by slot, image), tts status per selected text, distractors with each one's `core_ready`, recent events |
| POST | `/words` | Manual word creation `{lemma, role}` (rare; promotion normally via OOV resolve) |

## Candidates & selections

Kind ∈ `definition | example | image`.

| Method | Path | Purpose |
|---|---|---|
| POST | `/candidates/definition` | Mint manual candidate `{word_id, pos, text, parent_cand_id?}` |
| POST | `/candidates/example` | `{word_id, text, hl_start, hl_end}` — offsets advisory: the store canonicalizes the text and recomputes the highlight via `locate`; a sentence the word cannot be located in is refused with 422 `unprocessable` |
| POST | `/candidates/image` | multipart upload `{word_id, file}` → stored content-addressed |
| POST | `/candidates/{kind}/{cand_id}/reject` | Sets status=rejected (triggers pin-fallback if selected) |
| DELETE | `/candidates/example/{cand_id}` | Purge: hard-deletes the candidate row (admin erasure, distinct from reject). 409 while any slot points at it; writes `candidate_purged` event |
| POST | `/selections/{kind}` | Override selection: `{word_id, pos?/slot?, cand_id}` → selected_by=human, pinned=1. 409 when the target candidate is not `available` |
| POST | `/selections/{kind}/approve` | `{word_id, pos?/slot?}` → approved=1, records approved_hash. 409 when the slot's current candidate is not `available` |
| DELETE | `/selections/{kind}/approve` | Un-approve |
| POST | `/selections/definition/primary` | Move is_primary: `{word_id, pos}` |
| POST | `/selections/definition/enabled` | `{word_id, pos, enabled}` |

## Distractors

| Method | Path | Purpose |
|---|---|---|
| POST | `/distractors/rebind-violations` | `{dry_run}` (default `true`) → audit every binding for morphological pairs and repair them: `{scanned, violations, planned_or_applied: RebindItem[], unresolvable: RebindItem[], planned_or_applied_total, unresolvable_total, truncated, applied, skipped}` |

`RebindItem` = `{word_id, lemma, rank, old: {word_id, lemma}, new: {word_id, lemma} | null, core_ready_new}`.

Distractors are bound once and never change; this endpoint is the one
exception the product rule always allowed — a human replacing a binding that
should never have been made. A row is a violation when the word and its
distractor share a stem (`adapt`/`adapter`, `invest`/`investor`), which
bindings written before the exclusion rule still contain. Replacements follow
the live selection rule (same-POS, nearest by edit distance, ties on
frequency then id) and are drawn from `core_ready` words only, so a repair
never costs a word its place in the release; when nothing shippable is near
enough, the binding is listed under `unresolvable` and left alone. `dry_run`
mutates nothing and an absent body is a dry run. Applying writes one
`distractor_bound` event per moved row with the old and new ids and
`reason: "stem_violation_rebind"`; `skipped` counts ranks the writer declined
because the table had drifted since the plan was computed. Arrays cap at 500
entries per list (`truncated` says so); the events are never capped.

## OOV queue

| Method | Path | Purpose |
|---|---|---|
| GET | `/oov?status=open&page=` | Queue rows + occurrence contexts (which words/definitions contain the lemma) |
| POST | `/oov/{lemma}/resolve` | `{mode: "promote"}` → insert auxiliary word; `{mode: "rewrite", def_cand_id, text}` → mint rewrite candidate + select it. Both close the queue row |

## Dead letters

| Method | Path | Purpose |
|---|---|---|
| GET | `/dead-letters` | job_state rows with status=dead, joined with subject context |
| POST | `/dead-letters/retry` | `{kind, subject_type, subject_id}` → delete job_state row |
| POST | `/dead-letters/waive` | Same key → status=waived (enables fallback rules) |

## Plan & releases

| Method | Path | Purpose |
|---|---|---|
| GET | `/plan` | Current plan summary: stats, group list, diff-vs-previous counts |
| GET | `/plan/groups/{seq}` | Words of one group in order |
| GET | `/releases` | Release history |
| GET | `/releases/preview` | Holdback report: excluded words with root cause + downstream impact count, sorted by impact |
| POST | `/releases/export` | Trigger export; 409 if validation gates fail (body lists failures) |

## Media

| Method | Path | Purpose |
|---|---|---|
| GET | `/media/{file_hash}` | Serve bytes (image preview / audio playback), correct Content-Type, immutable cache headers |
