# Admin API Contract (morphod ⇄ admin-ui)

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
| POST | `/candidates/example` | `{word_id, text, hl_start, hl_end}` |
| POST | `/candidates/image` | multipart upload `{word_id, file}` → stored content-addressed |
| POST | `/candidates/{kind}/{cand_id}/reject` | Sets status=rejected (triggers pin-fallback if selected) |
| POST | `/selections/{kind}` | Override selection: `{word_id, pos?/slot?, cand_id}` → selected_by=human, pinned=1 |
| POST | `/selections/{kind}/approve` | `{word_id, pos?/slot?}` → approved=1, records approved_hash |
| DELETE | `/selections/{kind}/approve` | Un-approve |
| POST | `/selections/definition/primary` | Move is_primary: `{word_id, pos}` |
| POST | `/selections/definition/enabled` | `{word_id, pos, enabled}` |

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
