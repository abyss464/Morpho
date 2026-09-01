# core/store

22 specs.


============================================================
core/crates/store/src/
============================================================


--- conn.rs ---

# Database Connection Factory

Opens SQLite connections according to the work contract, automatically setting all required PRAGMAs (WAL, sync, foreign keys, busy wait).

## open_write_connection(path) -> Connection
Opens the single read-write connection. Across all processes, only write tasks hold this one connection. Enables WAL journal mode, NORMAL sync, and common PRAGMAs (foreign_keys, busy_timeout).

## open_read_connection(path) -> Connection
Opens a read-only connection for the read pool. In addition to the common PRAGMAs, it also sets query_only = true—even if write operations are written in SQL, they will be rejected.

## BUSY_TIMEOUT_MS
Busy wait limit, fixed at 5000 milliseconds.

## Constraints
- Absolutely do not call open_write_connection outside of write tasks; the entire process only allows one read-write connection.
- Do not skip the PRAGMAs here and directly use rusqlite to open a bare connection; otherwise, contract items such as foreign key constraints and WAL mode will all fail.


--- error.rs ---

# Storage Layer Error Types

All errors that can occur in the store layer are uniformly defined here.

## StoreError enum
- Sqlite — Underlying database error
- Io — File system error
- Json — Serialization/deserialization error
- Enum — Enum parsing failure
- NotFound(description) — The searched row does not exist
- Conflict(description) — The request is valid but violates business rules (similar to HTTP 409)
- Invalid(description) — The request format itself is incorrect (similar to HTTP 400)
- Unprocessable(description) — The request format is correct but the content cannot be processed (similar to HTTP 422)
- Closed — Write task already closed, shutting down
- Background(description) — Background database task crashed or was cancelled

## Convenience constructor methods
not_found(what), conflict(what), invalid(what), unprocessable(what) — accept any parameter that can be converted to a string, and directly construct the corresponding variant.

## Result alias
`Result<T>` is `std::result::Result<T, StoreError>`, uniformly used within the store layer.


--- lib.rs ---

# store crate entry point

Declares submodules and uniformly re-exports all public APIs. The writer module is private.

## Grouped by usage

**Connection & Pool:**
ReadPool, DEFAULT_READ_POOL_SIZE, Store, StoreConfig, WriteOutcome

**error:**
Result, StoreError

**mode & migration:**
ensure_schema, migrate, WORKING_DB_SQL

**media files:**
MediaStore, StagingDir, StoredMedia

**write operation types (ops submodule):**
WriteOp and the payload structs of all its variants — ImportWords, CreateWord, IngestDefinitions, IngestExamples, IngestImages, MintExampleCandidate, MintImageCandidate, MintDefinitionCandidate, SetSelection, SetApproval, ApplyAutoSelections, AutoSelection, ApplyScores, ApplyClipScores (via the ApplyScores export), ScoreUpdate, SetEtymology, SetAuxStatus, SetGloss, RecordDefExtraction, RecordTtsAsset, WritePlan, ApplyReadiness, MarkMediaGc, RecordRelease, BindDistractors, DistractorBinding, UpsertJobState, SyncOosQueue, OovResolution, MediaRegistration, ReadinessRow, PlanGroupRow, PlanWordRow, ImportStats, WriteResult


--- media.rs ---

# Content-Addressable Media Store

Manages file storage under `data/media/`. Files are sharded by hash (`media/{first two chars of hash}/{hash}.webp|.ogg`); identical bytes are stored only once. This module only handles file system operations and does not touch the data database.

## MediaStore

### new(data_dir)
Creates a handle pointing to the `data/` directory.

### root() -> path
Returns the absolute path of `data/media`.

### rel_path(file_hash, kind) -> string
Computes the relative path, e.g. `media/ab/abcdef...webp`.

### path_of(file_hash, kind) -> path
Returns the absolute path of an already stored file.

### put_bytes(bytes, kind) -> StoredMedia
Writes byte content into the store. First computes the hash; if identical content is already stored, returns directly (created = false). When writing, first saves to a `.part` temporary file, then atomically renames it, so no half-written files appear.

### put_file(source, kind) -> StoredMedia
Moves the adapter-produced file into the store. Empty files are rejected. Internally reads all bytes, then goes through put_bytes.

### contains(file_hash, kind) -> boolean
Checks whether the store really has this file (disk-level confirmation).

### staging(token) -> StagingDir
Creates a temporary working directory for one adapter call, returning a StagingDir handle. The directory is automatically cleaned up when the StagingDir is dropped.

### clean_staging() -> number cleaned
Called by the janitor at startup; deletes all leftover temporary directories (from a previous crash).

## StoredMedia
Result of storing into the store: file_hash, kind, rel_path, bytes, created (whether newly created).

## StagingDir
Temporary directory handle. `out_path(name)` gives the file path the adapter should write to. When dropped, the entire directory is automatically deleted.

## Constraints
- This module never deletes media files already in the store. Media GC only marks entries in the data database and does not delete files here.
- Do not bypass put_bytes / put_file and write files directly into media/ — that bypasses the hashing and sharding logic.


--- queries.rs ---

# Read-Only Query Collection

CLI, reconciler, exporter, and manage API share the read-side queries. All functions receive a read-only Connection and write nothing.

## Statistics

### word_counts(conn) -> WordCounts
Global word counts: total, target/base/auxiliary (active only), retired auxiliary words, active words, ready words, core ready words, blocked words. Gloss anchors (words with non-empty zh_gloss) are not counted in active/ready/blocked.

### asset_counts(conn) -> AssetCounts
Per-asset-slot summary: definition, example, and image each have ready/missing/failed counts. TTS fields default to zero and are populated separately by the caller. Also contains totals for the three candidate types.

### oos_open_count(conn) / dead_letter_count(conn) -> counts
Count of unresolved out-of-scope word items / number of dead letter tasks.

## Word Queries

### all_words(conn) -> full word list
Stably sorted by (frequency_rank, word_id).

### active_words(conn) -> active word list
Only active words (excluding gloss anchors), also stably sorted. This is the factory's working set — derive, selection, planning, distractors, readiness, and the published word database all fan out from here.

### word_by_id(conn, word_id) / word_id_by_lemma(conn, lemma) -> single word
Lookup by ID or word item. Lemma lookup is case-insensitive.

### gloss_anchors(conn) -> gloss anchor list
All words with non-empty zh_gloss, in ascending order.

## Dependencies and Distractors

### dependency_edges(conn) -> (word_id, depends_on_word_id) list
Definition dependency edges, with gloss anchor edges already removed.

### gloss_anchor_refs(conn) -> (word_id, anchor_word_id) list
Edges deliberately discarded by dependency_edges; the exporter uses them to package anchors.

### distractor_edges(conn) / distractor_pairs(conn)
Distractor bindings: edges return only IDs, pairs also include both sides' lemmas.

### primary_pos_map(conn) / word_pos_set(conn)
Primary part-of-speech map / set of parts of speech from all enabled senses.

## Out-of-Scope Words

### unresolved_definition_tokens(conn) -> (word_id, lemma) list
Word tokens in the selected definition that cannot be resolved to the words table.

### oos_lemmas(conn) -> out-of-scope word list
### pending_oos_words(conn) -> blocked word ID list
Words with unresolved out-of-scope words.

## TTS

### DESIRED_TTS_SQL
SQL fragment for the desired TTS set, containing the word itself, selected definition text, and selected example text; gloss anchors already excluded.

### tts_desired(conn) -> (kind, text) list
### tts_assets(conn) -> asset table indexed by input_hash
### abandoned_tts_inputs(conn) -> set of already-abandoned input_hashes
### tts_given_up(input_hash, assets, abandoned) -> boolean
Determines whether the engine has already given up on a TTS input (asset rows marked failed, or job_state marked dead/waived).

## Tasks and Rate Limits

### job_states(conn) -> all task status rows
### rate_limits(conn) -> rate limit configuration
### source_fetches(conn, kind) -> completion marks indexed by (word_id, source)

## Plan

### current_plan(conn) -> current plan summary (including group count and word count)
### plan_placements(conn, plan_id) -> learning order indexed by word_id
### plan_group_types(conn, plan_id) -> group types indexed by group_seq

## Media GC

### referenced_media(conn) -> list of file_hashes in use
### media_registry(conn) -> all media files and GC timestamps

## Constraints
- These functions are all read-only; do not perform any write operations here.
- NOT_ANCHORED is the unified predicate for filtering gloss anchors; new queries must use it when anchors need to be excluded.


--- read.rs ---

# Read-only connection pool

Uses a semaphore + mutex to manage a set of read-only SQLite connections. Each read operation is executed on a blocking thread via spawn_blocking, and the semaphore ensures that concurrency does not exceed the pool size.

## ReadPool

### open(path, size) -> ReadPool
Creates a pool, opening `size` read-only connections (at least 1).

### size() -> number of connections currently available

### with(f) -> result
Executes the closure `f` on a connection in the pool. First acquires a semaphore permit, retrieves a connection, executes the closure in spawn_blocking, then returns the connection and permit after completion. If the blocking task panics, the connection is lost but the permit is returned; the next time a connection is requested from an empty pool, a new connection will be reopened, so there will be no deadlock.

## DEFAULT_READ_POOL_SIZE
Defaults to 6 connections.

## Constraints
- Do not perform write operations inside the `with` closure — the connections are query_only and will error directly.
- Do not manually manage connection lifetimes; always use them via `with`.


--- schema.rs ---

# Schema Bootstrap and Migrations

Manages DDL creation and version migrations for the working database. The canonical DDL comes from `docs/contracts/working-db.sql`, embedded at compile time, with no separate copy kept.

## ensure_schema(conn) -> whether created

Core entry point. If the database is empty, creates all tables from the contract DDL and then runs the migration ladder; if the database already exists, runs migrations only. Either path seeds default rate-limit rows. Returns true to indicate this call created the database.

## migrate(conn) -> steps climbed

Walks the migration ladder upward from the current user_version to the latest version. Returns 0 if already at the latest. A database version newer than the code is rejected with an error.

## WORKING_DB_SQL

The full canonical DDL embedded at compile time.

## CONTRACT_RATE_LIMITS

Default rate configurations for all dispatch channels (rate_key, concurrency limit, per-minute refill, burst limit). Written with INSERT OR IGNORE, so operator adjustments are not overridden.

## seed_rate_limits(conn)

Seeds missing rate-limit rows. Existing rows are left untouched, so operator tuning survives.

## MIGRATIONS ladder

Each Migration describes a one-step upgrade (from version -> from+1) and supports three kinds of changes:

- add_columns — add columns (skipped if already present)
- rebuilds — rebuild tables (SQLite's 12-step procedure, used for things ALTER TABLE cannot do, such as changing CHECK constraints)
- creates — create tables (DDL taken from the contract file)

Each step is idempotent — changes already present are skipped. When rebuilding a table, all views are saved first and restored after the rebuild, and foreign-key integrity is checked before committing.

## Constraints

- The contract file's version may lag behind the code, but can never be ahead. A static assertion at compile time guarantees this.
- Adding a migration step requires implementing its inverse operation (revert) as well; the historical-version factory used in tests depends on it.
- Do not modify user_version manually; it is entirely managed by migrate.
- During table rebuild, foreign_keys is temporarily disabled; it must be restored afterward, and integrity checked via foreign_key_check.


--- store.rs ---

# Store Handle

The unique entry point for all system access to the working database. Cloneable; internally uses Arc to share state.

## StoreConfig

Constructor parameters: data database path (path), read pool size (read_pool_size, default 6), write queue depth (write_queue_depth, default 256), change bus capacity (change_bus_capacity, default 1024).

## Store

### open(config) -> Store

Opens (creating if necessary) the data database, starts the write task thread, and initializes the read pool and change broadcast bus. Automatically creates parent directories.

### path() -> data database file path

Returns the data database file path.

### was_created() -> boolean

Whether the data database file was created in this process.

### write(actor, op) -> WriteOutcome

Submits a write operation. Waits for the transaction to commit before returning the result. Each operation is an independent transaction.

### read(f) -> result

Executes a read-only query on one of the connections in the read pool.

### subscribe() -> change event receiver

Subscribes to the change bus. Slow consumers will be dropped if they fall behind, never blocking the write task.

### subscriber_count() -> current subscriber count

Current subscriber count. For diagnostics.

## Constraints

- Don't bypass Store to directly open data database connections.
- Don't attempt write operations inside read closures.
- Writes are queued asynchronously; don't assume the call completes immediately.


--- writer.rs ---

# Database Write Channel

The working database's only write entry point. All write operations in all projects are queued and executed here.

## submit(sender, actor, op) → WriteOutcome

Submit a write operation to the queue, wait for it to finish executing, and get the result back. Each operation is an independent transaction — either it all succeeds or nothing happens. On a successful write, all modules listening for changes (reconciliation loop, API push, etc.) are notified automatically; the caller doesn't need to manually notify anyone.

The returned WriteOutcome contains the operation result and the list of affected entities.

## spawn(conn, path, queue_depth, bus) → sender

Start the database writer thread and return a sender for submit to use. Called only once at process startup.

## Constraints

- Bypassing this channel to write directly is a data race and will crash.
- All write operation types are defined in ops/mod.rs; don't add new ones elsewhere.
- Don't assume write operations complete synchronously — they are queued and executed asynchronously.


============================================================
core/crates/store/src/ops/
============================================================


--- candidates.rs ---

# Candidate ingest operations

Example and image candidate writes, plus the bulk ingest path used by fetch executors. An ingest is one atomic write: every candidate the source returned and the `source_fetch` completion marker land in the same transaction. That makes a legitimately empty answer terminal -- the rule queries the marker, never "does a candidate exist". Handles deduplication by text_hash/file_hash for definitions, examples, and images.


--- clip.rs ---

CLIP similarity score writes. One row = one (picture, text, model) triple. Content-addressed: two words sharing the same photograph and sentence share one row. A model_ver change writes new rows; old ones lose their readers. Re-scoring the same triple is an in-place update (GPU is not bit-exact across drivers).


--- derived.rs ---

Derived-artifact writes: tokenization cache (`def_extractions`), fetch completion markers (`source_fetches`), and the media registry (`media_files`). Engine bookkeeping, so no audit-log rows are appended. Implements optimistic concurrency: a result carries the input hash it was computed from and is discarded if the input drifted.


--- distractors.rs ---

Distractor binding writes. `BindDistractors` only inserts rows for words that lack them -- no automatic path rewrites a binding. `RebindDistractors` is the explicit human action: takes a reason, records the actor in `bound_by`, and writes one event per moved row. Never reachable from the reconciler.


--- jobs.rs ---

`job_state` writes. The queue is never persisted; this table only remembers failure states that survive a restart. A successful run deletes the row -- no row means healthy. Operations: `UpsertJobState` (persist backoff/dead/waived state), `ClearJobState` (delete on success), `WaiveJob` (set status=waived, arms fallback rules).


--- mod.rs ---

# Write operations module root

Every `WriteOp` is exactly one SQLite transaction, applied by the single writer task. Audit-log rows are appended inside the transaction so a state change and its `events` row are never observed apart. `WriteOp::Batch` composes several operations into one transaction. Defines the `OpCtx` context struct, `WriteResult` return type, and the `WriteOp` enum dispatching to all typed operations.

## Constraints
- All writes go through this channel; bypassing it = data race.
- Do not add new operation types elsewhere.


--- oov.rs ---

OOV queue resolution writes. Two resolution modes: `ResolvePromote` inserts the lemma as an auxiliary word with a definition candidate and selects it; `ResolveRewrite` mints a new `llm_rewrite` candidate linked to its parent and selects it. Both are pure state writes; the engine derives every consequence.


--- plan.rs ---

Learning-plan artifact writes. A plan is a versioned singleton: the new artifact, groups, and word placements all land in one transaction, and `is_current` moves atomically. Old artifacts are kept for diff. Optimistic concurrency: if a plan with the same `input_hash` is already current, the write is a no-op.


--- readiness.rs ---

Readiness cache writes for `words.ready`, `words.core_ready`, `words.blockers`. These columns are a cache, not a source of truth -- recomputed inline every pass. Only rows whose computed value actually changed are touched, keeping the change bus quiet once the system has converged.


--- release.rs ---

Release bookkeeping writes. The `releases` row and its `release_manifests` rows land together: the manifest pins every media file a shipped APK references, and GC must never observe a release without its pins. Records version, plan_id, input_hash, db_file_hash, exported_by, notes, and derived counts (word_count, media_count, total_bytes).


--- selections.rs ---

# Selection and approval operations

Candidate minting, slot selection, approval and rejection. Candidates are immutable -- an "edit" mints a new candidate. Changing which candidate a slot points at bumps `selection_rev` and invalidates approval. Approval implies a pin and records the approved content hash. Rejecting the selected candidate clears the pin and approval so auto-selection can fall back on the next cycle. Operations: `MintDefinitionCandidate`, `SetSelection`, `ApproveSelection`, `UnapproveSelection`, `RejectCandidate`, `SetPrimary`, `SetEnabled`.


--- tts.rs ---

TTS asset writes. Keyed by what was synthesized (`input_hash`), not by the candidate that requested it. A different text produces a different row. The only in-place transition is `failed -> ready` on retry success. The media registration and the asset row share one transaction so there is never a `tts_assets` row pointing at a nonexistent `media_files` row.


--- words.rs ---

Word-list import and single-word creation. `ImportWords` bulk-imports a word list with role and created_by. `CreateWord` creates a single word (used by manual creation and OOV promotion). `SetGloss` records etymology text and source on an existing word.
