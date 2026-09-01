# core/api

8 specs.


============================================================
core/crates/api/src/
============================================================


--- dto.rs ---

# API Data Transfer Objects

Definitions of the JSON structures for all admin API requests and responses. Each structure corresponds one-to-one with the interfaces in the frontend `admin-ui/src/api/types.ts`. SQLite's 0/1 integers are converted to boolean values here, and JSON strings stored in TEXT columns are decoded into objects on the service side.

## Pagination

### Page\<T\>
The wrapper for paginated responses. Contains `items` (the data list of the current page) and `total` (the total count matching the item criteria).

### Pagination
Parsed from query parameters `?page=&page_size=`. Page numbers start at 1, defaulting to 50 items per page, with a maximum of 200 items. Provides `limit()` and `offset()` methods for SQL use.

## Dashboard

### Dashboard
Dashboard overview, containing vocabulary statistics (DashboardWords), asset statistics (DashboardAssets, with ready/missing/failed counts for each of the four categories: definition/example/image/TTS), open OOV count, dead letter count, current plan summary, and recent event list.

## Vocabulary

### WordListItem
A row in the vocabulary list, containing word ID, word form, role, ready status, blocking item list, whether it has an image, definition count, example count, and missing TTS count.

### Word
The complete fields of a word. A non-empty `zh_gloss` indicates this is a comment anchor — it does not participate in asset collection, does not occupy a plan slot, and is not counted toward readiness; all chain items depending on it are automatically satisfied.

### WordDetail
The complete data package for the word detail page, containing the Word entity itself, definition slot list grouped by part of speech, three example slots, image slot, TTS status list, distractor word list, and recent events.

## Candidates and Selections

### DefinitionCandidate / ExampleCandidate / ImageCandidate
Candidate items for the three types of assets. Each item contains source, score, score details (JSON object), status, etc.

### DefinitionSelection / ExampleSelection / ImageSelection
Records of the current selections for the three types of assets. Contain `pinned` (whether locked against automatic replacement), `approved` (whether manually approved), approval hash, etc.

### DefinitionSlotView / ExampleSlotView / ImageSlotView
Assemble the selection record and candidate list of a slot together. Definitions are grouped by part of speech, examples have three fixed slots (1-3), and images have only one slot.

## TTS

### TtsStatusView
The speech synthesis status of an item's text. Status values are ready / failed / missing. The `ref` field identifies whether it belongs to a selected definition (pos) or example (slot); it is empty for the word itself.

## Distractor Words

### DistractorView
A distractor word bound to a word, containing rank, ready status, and binding time.

## OOV Queue

### OovQueueEntry
An out-of-vocabulary word entry, containing status, first discovered time, and occurrence position list (OovOccurrence). Each occurrence position also carries the latest available LLM rewrite suggestion.

## Dead Letter

### DeadLetter
A record of a failed task, containing task kind, subject type/ID, retry count, next retry time, and last error. The `subject` field provides a human-readable context label.

### JobKeyBody
The request body submitted when retrying or exempting a dead letter, specifying the task's kind/subject_type/subject_id triple.

## Plan

### PlanSummary
Summary of the current learning plan, containing plan ID, algorithm version, build parameters, statistics, group list, and differences from the previous plan version (added/removed/reordered counts).

### PlanGroupDetail
Detailed list of all words in a group, each word with learning sequence number, ready status, and blocking items.

## Publishing

### Release
An export record, containing version number, plan ID, content hash, word count, media file count, and total bytes.

### ExportBody / PublishBody
Export and publish request bodies. PublishBody additionally accepts `no_build` to skip the Gradle build.

## Mutation Request Bodies

### CreateWordBody
Create vocabulary: word form, role (required), pronunciation, frequency ranking (optional).

### MintDefinitionBody / MintExampleBody
Manually create definition/example candidates.

### SelectionBody
Override selection: specify word ID, slot (definition requires pos, example requires slot 1-3, image does not need one), and candidate ID. `pin` defaults to true; once locked, the automatic flow will not replace it.

### ApprovalBody / SetPrimaryBody / SetEnabledBody / SetGlossBody
Request bodies for approval, setting the primary definition, enabling/disabling definition slots, and setting the Chinese gloss.

### OovResolveBody
Resolve an out-of-vocabulary word with one of three modes: promote (promote to a formal word), rewrite (rewrite the definition that references it), gloss (replace with a Chinese gloss).

### RebindViolationsBody
Rebind violating distractor words. `dry_run` defaults to true; a missing request body is also treated as a dry run.

## Query Filters

### WordListQuery / EventQuery / OovQuery / GalleryQuery / DeadLetterQuery / PageQuery
Query parameters for each list endpoint. All embed pagination parameters and provide a `pagination()` method. WordListQuery supports filtering by role, ready status, blocking items, group, and keyword. GalleryQuery supports filtering by source, approval status, and keyword, as well as sorting by CLIP similarity in ascending or descending order.

## Utility Functions

### decode_json(raw) → optional JSON value
Decodes a JSON string stored in a SQLite TEXT column into an object. Returns empty if parsing fails or the value is empty.

## Constraints
- The structures here must maintain one-to-one correspondence with `admin-ui/src/api/types.ts`; modifying one side unilaterally will cause frontend-backend inconsistencies.
- SQLite's 0/1 must be converted to boolean values, and JSON string columns must be decoded into objects — never pass raw strings through to the frontend.


--- error.rs ---

# API error envelope

takeall API errors are uniformly wrapped in the JSON format `{"error": {"code": "...", "message": "..."}}` and returned with the corresponding HTTP status code.

## ApiError

The error type at the API layer, containing the HTTP status code, machine-readable error code, and human-readable message. Automatically implements HTTP response conversion: 500-level errors are logged at error level, others at debug level.

### Convenience constructors

- `not_found(message)` → 404 `not_found`
- `bad_request(message)` → 400 `invalid_request`
- `conflict(message)` → 409 `conflict`
- `unprocessable(message)` → 422 `unprocessable` — request format is correct but content has issues
- `internal(message)` → 500 `internal`
- `not_implemented(path)` → 501 `not_implemented` — endpoints declared in the contract but not implemented in the current build
- `export_conflict(failures)` → 409 `export_gates_failed` — export pre-checks did not pass; the response body additionally contains a `failures` array, listing each failed gate and its reason

### Automatic conversion

StoreError is automatically mapped: NotFound → 404, Conflict → 409, Invalid → 400, Unprocessable → 422, Closed → 503, everything else → 500. rusqlite errors are uniformly mapped to 500.

## ApiResult\<T\>

Type alias for `Result<T, ApiError>`, the return type of all handlers.

## constraint

- All API errors must use this envelope; don't hand-craft JSON responses.
- The `failures` field is only used in export conflict scenarios; don't attach it to other errors.


--- lib.rs ---

# API crate entry point

HTTP service entry point for the morphod admin backend. Implements the interface defined in `docs/contracts/admin-api.md`, mounted under the `/api` path; also provides frontend static file service when the `admin-ui/dist` directory exists.

## Modules

- `dto` — request/response JSON data structures
- `error` — unified error envelope
- `queries` — read-only database queries
- `routes` — HTTP routes and handler functions
- `state` — shared application state

## Re-exports

- `ApiError`, `ApiResult` — re-exported from the error module
- `AppState` — re-exported from the state module

## build_router(state, admin_ui_dist) → Router

Builds the complete HTTP service. API routes are mounted under `/api`. If `admin_ui_dist` points to an existing directory, static files are served in single-page application mode (unknown paths fall back to `index.html`); otherwise the root path returns plain text indicating the service is already running. Request logs are automatically recorded via tower's `TraceLayer`; 501 responses are treated as normal and do not trigger error-level logs.

## Constraints

- Mutations are only executed through the WriteOp queue; the API layer must not write to the database directly.


--- queries.rs ---

# API Read-Only Queries

All admin API GET endpoints are backed by database queries. All are executed on read-only connections, dispatched through `Store::read`.

## Dashboard

### dashboard(conn, tts_config) → Dashboard
Aggregates global statistics: word counts (total/target/auxiliary/ready/blocked), four types of material override rates (definition/example/image/TTS each with ready/missing/failed), open OOV count, dead letter count, current plan summary, and the latest 20 events. The TTS override rate is determined by computing the content address based on the configured voice parameters.

## Word List

### word_list(conn, query, tts_config) → Page\<WordListItem\>
Paginated query of the word list. Supports filtering by role, ready status, blocked item type, plan group, and word-form keyword. Sorting method: frequency rank first, then word ID for ties. Each word additionally computes the count of missing TTS items.

## Gallery

### gallery_list(conn, query) → Page\<GalleryItem\>
Paginated query of the gallery view of already-selected images. Only shows target/auxiliary words that have selected images. Supports filtering by source, approval status, and word-form keyword. Supports ascending CLIP semantic similarity (worst match first, used for review) or descending order — when sorting is enabled, all data is loaded and sorted in memory before pagination. Defaults to sorting by frequency rank. Each image includes its CLIP similarity score with the word query text and slot 1 example.

## Word Details

### word_fields(conn, word_id) → Word
Queries the complete fields of a word. Returns a NotFound error if not found.

### word_detail(conn, word_id, tts_config) → WordDetail
Assembles all data for the word detail page: word fields, definition slots (grouped by part of speech, main senses first), three example slots (always returns items 1-3, even if empty), image slots, TTS status, distractor words, and related events. Event scope covers this word and all its candidate/selected/distractor word entities.

## Events

### recent_events(conn, limit) → Event list
Gets the most recent N events in reverse chronological order.

### events_page(conn, query) → Page\<EventRecord\>
Paginated event query, supports filtering by entity type, entity ID, and action.

## OOV Queue

### oov_page(conn, query) → Page\<OovQueueEntry\>
Paginated query of the out-of-vocabulary word queue, supports filtering by status. Each word item includes a list of occurrence positions and the latest LLM rewrite suggestion.

## Dead Letter

### dead_letters(conn, pagination, rate_key) → Page\<DeadLetter\>
Paginated query of failed tasks that are dead. Supports filtering by rate_key to specify a dispatch channel; an unrecognized rate_key returns empty results rather than an error. Each record includes a human-readable subject label (resolve_subject resolves word name/definition summary/TTS text, etc.).

### resolve_subject(conn, subject_type, subject_id) → DeadLetterSubject
Takes a task subject ID and resolves it to human-readable information. Supports three types: word (including fan-out with `:source` suffix), def_candidate, and tts_input; the rest return the original ID.

## Plan

### plan_summary(conn) → optional PlanSummary
Current plan summary, containing statistics, group list, and differences from the previous version. Returns empty when there is no plan.

### plan_group(conn, group_seq) → optional PlanGroupDetail
All words in a group of the current plan, ordered by learning sequence number.

## Releases

### releases(conn, pagination) → Page\<Release\>
Paginated query of release history, in reverse time order. Each item contains version number, word count, media file count, and total bytes.

## Media

### media_file(conn, file_hash) → MediaRow
Queries the media file's type, relative path, and size by content hash. Returns NotFound if not found.

### job_labels(conn) → Label mapping
Provides human-readable labels for jobs endpoints: word ID → word form, including expansion of source suffixes.

## Constraints
- All queries can only run on read-only connections; do not perform any write operations here.
- The TTS status determination rules (ready/failed/missing three states) must use the same logic as the dashboard summary; there must be no contradiction between word details and dashboard numbers.


--- state.rs ---

# Shared Application State

All handlers share the runtime state. It is injected into each request handler via axum's `State` extractor.

## USER_HEADER

Constant `"x-morpho-user"`. The request identifies the operator's identity via this header. There is no authentication mechanism; the API is designed to be bound only to localhost.

## AppState

Contains the following resources:

- `store` — database read/write channel
- `jobs` — in-flight task registry (in-memory)
- `data_dir` — path to the `data/` directory, used for resolving relative media file paths
- `releases_dir` — output directory for export packages
- `repo_root` — repository root directory, used by the release process to locate the Android project tree
- `default_user` — default operator name, defaults to `"local"`
- `tts` — TTS voice configuration, used for computing content addresses
- `media` — media file storage
- `export` — export settings

### new(store, jobs, data_dir, export) → AppState

Constructs the state. `releases_dir` defaults to `data/releases`, `repo_root` defaults to the current directory. TTS configuration is extracted from the export settings.

### with_releases_dir(dir) → AppState

Overrides the releases directory.

### with_repo_root(dir) → AppState

Overrides the repository root directory.

### actor(headers) → Actor

Extracts the operator identity from the request headers and wraps it as an `Actor` for the admin type.

### user(headers) → username

Reads the username from the `X-Morpho-User` header. Uses `default_user` when it is empty or missing.

## Constraints

- Don't add authentication logic at the API layer — the admin API relies on network binding (localhost only) to enforce access control.
- `data_dir` is used for path resolution; media endpoints must ensure that the resolved path does not escape this directory.


============================================================
core/crates/api/src/routes/
============================================================


--- mod.rs ---

# API Route Registration

Table of routes for all admin API endpoints. All endpoints declared in this contract are already implemented; no 501 responses.

## Modules

- `read` — read-only endpoints (GET)
- `write` — mutation endpoints (POST / DELETE)

## api_router(state) → Router

Returns the full route tree mounted under `/api`. Endpoints are grouped by function:

**Dashboard & Observability**
- `GET /dashboard` — overall statistics
- `GET /events` — event log
- `GET /jobs` — task queue snapshot
- `GET /stream` — SSE change stream

**Gallery**
- `GET /gallery` — selected image gallery

**Vocabulary**
- `GET /words` — vocabulary list
- `POST /words` — create vocabulary
- `GET /words/{id}` — vocabulary details
- `POST /words/{id}/gloss` — set Chinese gloss
- `DELETE /words/{id}/gloss` — clear Chinese gloss

**Candidates**
- `POST /candidates/definition` — create definition candidate
- `POST /candidates/example` — create example candidate
- `POST /candidates/image` — upload image candidate (multipart)
- `DELETE /candidates/example/{cand_id}` — permanently delete example candidate
- `POST /candidates/{kind}/{cand_id}/reject` — reject candidate

**Selection Management**
- `POST /selections/{kind}` — override selection
- `POST /selections/{kind}/approve` — approve selection
- `DELETE /selections/{kind}/approve` — revoke approval
- `POST /selections/definition/primary` — set primary sense
- `POST /selections/definition/enabled` — enable/disable sense

**Distractors**
- `POST /distractors/rebind-violations` — detect/fix root-violating bindings

**OOV Queue**
- `GET /oov` — out-of-vocabulary word list
- `POST /oov/{lemma}/resolve` — resolve out-of-vocabulary word

**Dead Letters**
- `GET /dead-letters` — failed task list
- `POST /dead-letters/retry` — retry dead letter
- `POST /dead-letters/waive` — waive dead letter

**Plan & Release**
- `GET /plan` — current plan summary
- `GET /plan/groups/{seq}` — group details
- `GET /releases` — release history
- `GET /releases/preview` — export preview
- `POST /releases/export` — perform export
- `POST /releases/publish` — one-click publish (export + sync + build)

**Media**
- `GET /media/{file_hash}` — get media file by hash

## Constraints
- Fixed path segments must be registered before wildcard paths (e.g., `definition/primary` must come before `{kind}`), otherwise the wildcard will swallow the fixed path.


--- read.rs ---

# read-only endpoints

All GET request handlers. Each handler obtains a read-only connection from AppState, calls the queries module to query, and returns JSON.

## dashboard() → Dashboard
Returns overall dashboard statistics.

## events(query) → Page\<EventRecord\>
Paginated query for event logs, with filtering by entity type/ID/action.

## words(query) → Page\<WordListItem\>
Paginated query of the word list, supporting filtering by role, ready status, blocked items, group, and keyword.

## gallery(query) → Page\<GalleryItem\>
Paginated query for the selected image gallery, with filtering by source/approval/keyword, and sorting by CLIP similarity.

## word_detail(word_id) → WordDetail
Returns all data for the word detail page.

## jobs() → JobsSnapshot
Returns a snapshot of the task queue: in-memory in-flight tasks plus tasks in the database that are in backoff status. Backoff tasks resolve into human-readable subject labels.

## stream() → SSE
Server-Sent Events push change notifications. Changes of the same type are batched within a 250ms window, and a heartbeat is sent every 30 seconds to keep the connection alive. Each frame is formatted as `event: change`, with the data being JSON `{"entity_type": "...", "entity_ids": [...]}`.

## media(file_hash) → file content
Gets the media file by a 64-bit hexadecimal hash. After validating the hash format, it looks up the file path and type from the table, verifies that the path does not escape the data directory, then reads and returns the file. Since content addressing is immutable, the response is set with a one-year cache.

## oov(query) → Page\<OovQueueEntry\>
Paginated query for the out-of-vocabulary word queue.

## dead_letters(query) → Page\<DeadLetter\>
Paginated query of the dead letter list, supporting filtering by scheduling channel.

## plan() → PlanSummary
Returns a summary of the current plan. Returns 404 when there is no plan.

## plan_group(group_seq) → PlanGroupDetail
Returns the word list for the specified group in the current plan. Returns 404 if the group does not exist.

## releases(query) → Page\<Release\>
Paginated query of release history.

## release_preview() → HoldbackReport
Preview export: evaluates which words will be kept/excluded without actually writing a file. Returns 404 when there is no plan.

## Constraint
- The media endpoint must validate that the file path does not escape the data directory, preventing path traversal.
- The SSE merge window and heartbeat interval are contractual and must not be changed arbitrarily.


--- write.rs ---

# Mutation Endpoints

All write-operation handler functions. Each handler takes the request body, translates it into a WriteOp, and submits it to the write queue. After a successful write, it reloads the affected resources for the response. Word-related changes uniformly return the complete WordDetail, because a single edit can cascade-affect selection, approval, and ready status.

## Lexicon

### create_word(body) → 201 WordDetail
Create a new word. The body requires lemma and role, with optional phonetic and frequency_rank.

### set_gloss(word_id, body) → WordDetail
Set a Chinese gloss anchor for the word. The gloss cannot be empty. Once set, the word no longer participates in material collection and plan ranking; all dependency chains are automatically satisfied.

### clear_gloss(word_id) → WordDetail
Clear the Chinese gloss; the word resumes its normal lifecycle.

## Candidate Materials

### mint_definition(body) → 201 WordDetail
Manually create a definition candidate. The source is marked as Manual.

### mint_example(body) → 201 WordDetail
Manually create an example candidate. The source is marked as Manual.

### upload_image(multipart) → 201 WordDetail
Upload an image candidate. Receives a multipart form containing word_id (text field) and file (file field, limit 24MB). The image goes through the same encoding pipeline as automatically acquired images (scale to target dimensions, convert to WebP, content-addressed storage), ensuring manually uploaded images are fully consistent with automatically collected images downstream.

### reject_candidate(kind, cand_id) → WordDetail
Reject a candidate. kind is definition/example/image. Reversible; the candidate remains visible.

### purge_example(cand_id) → WordDetail
Permanently delete an example candidate (rows/lines are deleted). Candidates already selected cannot be deleted; the slot must be moved first. Used to clear content that should not be stored.

## Selection Management

### set_selection(kind, body) → WordDetail
Override a slot's selection. definition requires pos, example requires slot (1-3), image requires no additional parameter. `pin` defaults to true; once locked, automatic processes no longer replace it.

### approve(kind, body) → WordDetail
Approve the current selection.

### unapprove(kind, body) → WordDetail
Revoke the approval.

### set_primary(body) → WordDetail
Set a part of speech as the primary sense.

### set_enabled(body) → WordDetail
Enable or disable a sense slot.

## OOV Queue

### resolve_oov(lemma, body) → Page\<OovQueueEntry\>
Resolve an OOV word, three modes:
- promote: elevate the OOV word to a formal word, with optional pronunciation and frequency rank
- rewrite: rewrite the definitions that reference this OOV word; a candidate ID and new text are required
- gloss: replace with a Chinese gloss; the gloss cannot be empty

When notes are included, an additional event item is recorded. The response returns the updated open-status OOV list, making it convenient for the frontend to refresh the page.

## Interference Words

### rebind_violations(body) → RebindReport
Detect and fix interference word bindings that violate word roots (such as adapt/adapter — this is what's called elimination mode rather than vocabulary pairing). `dry_run` defaults to true; an empty body or null is also treated as a dry run. Returns the total scanned count, violation count, the replacement list for planned/executed rows/lines, and the list that cannot be replaced (each array limited to 500 items).

## Dead Letter

### retry_dead_letter(body) → Page\<DeadLetter\>
Retry a dead letter — delete the job_state rows/lines so the scheduler rediscovers it. Also records one JobRetried event item. Returns the updated dead letter list.

### waive_dead_letter(body) → Page\<DeadLetter\>
Waive a dead letter — set the status to waived, indicating "this requirement will always be satisfied as missing". Returns the updated dead letter list.

## Publishing

### export_release(body) → JSON
Execute the export: run gate checks; after they pass, package release.db and media files into a timestamp-named directory. Gate failure returns 409 with failure details.

### publish_release(body) → JSON
One-click publish for the entire flow: export → sync release.db to the Android project → sync media files (incremental copy; obsolete files moved to the recycle bin) → update the version number in test cases → optionally run Gradle build to generate APK. When `no_build` is true, the build step is skipped.

## Constraints
- Every change must be translated into a WriteOp; do not bypass the write queue to directly operate on the database.
- Image uploads must go through the encoding pipeline; do not directly save raw bytes.
- The default behavior of rebind_violations' dry_run is safe (no writes); it must never be changed to default to write.
