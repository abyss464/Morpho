# core/domain

13 specs.


============================================================
core/crates/domain/src/
============================================================


--- blocker.rs ---

# ready Blocking Codes

Defines the transition from "not ready" to "publishable" — all blocking items must be cleared first. The blocking code vocabulary corresponds one-to-one with the admin backend frontend; any additions or removals must be synced on both sides.

## BlockerCode enum

16 codes, covering: missing/unapproved primary sense, missing definition / unresolved OOS, dependency not ready, missing/unapproved example, missing/unapproved image, missing/failed TTS, distractor word unbound/not ready, not in plan.

### as_str() → string
Returns the snake_case wire spelling of the code, e.g. `"missing_primary_sense"`.

### ALL
The canonical ordering of all codes, used for deterministic ordering during serialization.

### is_core() → boolean
Determines whether this code participates in `core_ready` computation. The four distractor-related codes do not participate — this is the key design that prevents deadlock between two words (adapt / adopt) that are each other's distractors.

### distractor_not_ready(rank) → optional code
Takes a rank of 1–3, returns the corresponding `distractor_N_not_ready` code; returns empty if out of range.

### string conversion
Supports Display and FromStr; unknown strings return ParseEnumError.

## BlockerSet

A deduplicated, ordered set of blocking codes, ultimately written to the `words.blockers` column.

### insert(code) / insert_if(condition, code)
Adds a code to the set (ignored if already present). insert_if is the conditional version.

### core_ready() → boolean
True when none of the codes in the set are core-level — i.e. distractor issues do not affect the core_ready determination.

### ready() → boolean
True when the set is empty, indicating the word is fully ready.

### sorted() → code list
Returns codes in ALL's canonical order, guaranteeing identical output regardless of insertion order.

### to_json() → string
Serializes the sorted result as a JSON array string, written directly to the database column.

### FromIterator collection
Implements FromIterator, so `.collect()` can be used to build a BlockerSet.

## parse_blockers(raw) → string list

Parses the JSON array stored in the database `words.blockers` column; returns an empty list if parsing fails.

## constraints
- When adding or removing blocking codes, the BlockerCode union type in `admin-ui/src/api/types.ts` must be updated in sync.
- The order of the ALL list determines the order of serialized output; changing the order affects the stability of stored values.
- The definition of core_ready (which codes are excluded) directly relates to the distractor recursion depth limit and must not be modified casually.


--- canon.rs ---

# Text Normalization

All text used in hash calculations must undergo this preprocessing before being stored and compared.

## canonicalize(input) → string

Performs three operations on the input text: NFC normalization, trims leading/trailing whitespace, and compresses internal consecutive whitespace (including NBSP, full-width spaces, tabs, and newlines) into a single ASCII space. Case is preserved (TTS is case-sensitive).

This function is idempotent: applying canonicalize to its result again produces unchanged output.

## fold_lemma(input) → string

Converts to lowercase on top of canonicalize. Used for word database matching and lookup — the lemma column in the database is COLLATE NOCASE, and the lemmatizer outputs lowercase, so the lookup key must also be lowercase.

## Constraints

- Any text that needs hashing or comparison must first pass through canonicalize; bypassing it will cause identical content to produce different hashes.
- Do not change the case-preserving behavior; TTS depends on it.
- Idempotency is an assumption of the storage layer; breaking it will cause inconsistent hash chains.


--- change.rs ---

# Change Bus Payload

After a database transaction is committed, publish the affected entity keys so subscribers (reconciliation loop, SSE push) can perform low-latency updates. Subscribers only use these notifications as acceleration hints; correctness is guaranteed by periodic full scans.

## EntityType enum

15 entity types: Word, DefinitionCandidate, DefinitionSelection, ExampleCandidate, ExampleSelection, ImageCandidate, ImageSelection, DefExtraction, OosQueue, TtsAsset, MediaFile, JobState, SourceFetch, Plan, Distractor, Release.

Supports as_str / Display / FromStr / serialization; unrecognized strings return ParseEnumError.

## ChangeEvent

A single-item change notification: one entity type plus the list of affected ids under that type. The structure aligns with the manage API's `GET /stream` format.

## ChangeSet

Change accumulator during transaction execution. Collects affected ids bucketed by entity type.

### touch(entity_type, id)
Records an affected entity. The same id is recorded only once.

### touch_many(entity_type, ids)
Records in batch.

### into_events() → change event list
Called after transaction commit; converts the accumulated results into a ChangeEvent list and hands them to the change bus for publishing.

## Constraint
- ChangeEvent's structure must remain consistent with `GET /stream` in admin-api.md.
- When adding a new entity type, the EntityType enum and the admin-ui frontend must be updated in sync.


--- error.rs ---

# Error Classification

Defines the retry behavior when a reconciliation task fails. The three-tier classification determines how the engine handles a failure.

## ErrorKind enum

Online serialization uses the three-tier classification: permanent, transient, rate_limited. The adapter returns this value via standard output.

## TaskError

The concrete failure of a reconciliation task.

### permanent(msg) → TaskError

Permanent failure — a legitimate empty result or a request that cannot be completed. Record a completion mark; never retry.

### transient(msg) → TaskError

Temporary failure — network interruption, 5xx, timeout. Retry after exponential backoff.

### rate_limited(retry_after) / rate_limited_ms(ms) → TaskError

Rate limit — the entire items channel is paused for the specified time. This attempt does not count toward the dead letter threshold.

### kind() → ErrorKind

Returns the classification of this error.

### counts_as_attempt() → boolean

Whether it counts toward `job_state.attempts`. Rate limits do not count as an attempt (the rate limit is a property of the channel, not of the task).

### is_retryable() → boolean

Whether it should be retried. Only permanent is non-retryable.

### message() → string

Writes the error description to `job_state.last_error`.

## Constraints

- The semantics of the three-tier classification must not be modified — the entire scheduler's retry logic is built on top of this classification.
- rate_limited does not count toward the attempt count. This is a design decision; changing it would cause rate limits to be misjudged as dead letters.


--- event.rs ---

# Audit Log Glossary

Structure of the events table and the action enum. Every human-visible status change writes one audit record in the same transaction, ensuring that "why the engine did this" is always traceable.

## Actor enum

Records who triggered an operation:
- Reconciler — the reconciliation loop itself
- Worker(JobKind) — the executor of a particular job kind
- Admin(username) — the caller of the manage API
- Cli — terminal subcommands
- System(name) — test fixtures and internal maintenance

### admin(user) → Actor
Convenience constructor for an Admin actor.

Display output format, e.g., `"reconciler"`, `"worker:extract_tokens"`, `"admin:abyss"`.

## Action enum

29 closed action codes, covering: word import/creation, candidate add/remove/revive, selection change, approve/cancel approval/approval expiry, pin fallback, primary sense move, slot enable toggle, auxiliary word promotion/retirement, OOS open/close/auto-close, plan rebuild, distractor word binding, task death/exempt/retry, source fetch, word source setting, definition set/clear, media GC mark, publish export.

Closed enum — the manage backend frontend relies on this set for rendering, so no value outside the set may appear.

## EventRecord

`GET /api/events` returns one audit record. Fields: event_id, ts, actor, entity_type, entity_id, action, detail (JSON or null).

## EventDraft

The content a write operation attaches to the audit log.

### new(entity_type, entity_id, action) → EventDraft
Creates one draft item.

### detail(json_value) → EventDraft
Chainable; appends a JSON detail.

## Constraints
- Action is a closed set; new actions must be synced with the manage backend frontend.
- Audit records must be written in the same transaction as the change; writing them separately will cause the log to become inconsistent with the actual status after a crash.


--- hash.rs ---

# Hash Computation

The BLAKE3 hash entry point for all items. All hashes are 64-character lowercase hexadecimal strings. Each hash mixes in the version number of the algorithm that produced it, so upgrading a particular tool only causes the derivatives it produces to become outdated.

Fields are separated by a length prefix (u64 little-endian) to prevent collisions between `["ab","c"]` and `["a","bc"]`.

## HashInput

Incremental builder for scenarios where multiple fields are needed.

### new(algo_version) → HashInput

Specifies the algorithm version and starts a new hash. The version number is mixed in as the first field after the domain label.

### field(text) → HashInput

Appends a text field that is already in canonical form (e.g., another hash value).

### field_canonical(text) → HashInput

Appends a text field that needs to be canonicalized first; internally calls `canonicalize`.

### field_bytes(bytes) → HashInput

Appends a raw byte field.

### finish() → string

Completes the computation and returns a lowercase hexadecimal string.

## hash_fields(algo_version, fields) → string

Convenience function: given an algorithm version and a set of already-canonicalized fields, directly computes the hash.

## text_hash(text) → string

Content hash for any candidate text (definition, example, TTS input). The input is canonicalized first. The algorithm version is taken from the canonicalizer's own version.

## file_hash(bytes) → string

Content address for media files: pure BLAKE3 with no domain label or algorithm version mixed in — content addressing must remain stable forever; the same image downloaded from two sources must deduplicate to the same file.

## def_extraction_input_hash(text_hash, tokenizer_ver, lemmatizer_ver) → string

Input hash for definition extraction. A change in any of the three inputs produces a new hash.

## tts_input_hash(text, voice, engine, engine_ver, params_json) → string

Input hash for TTS synthesis. The text is canonicalized first. A change in any of the five inputs produces new asset rows/lines.

## Constraints

- `file_hash` must never mix in a version number or domain label; otherwise the same content would produce different addresses under different versions, and deduplication would break.
- Text passed to `field` must already be in canonical form; if in doubt, use `field_canonical`.
- Changing any algorithm version constant causes all derivatives of that type to be recomputed.


--- job.rs ---

# Task Vocabulary

All type definitions for the reconciliation scheduler: task kinds, subjects, channels, priorities, queue snapshots. The task queue itself is derived and not persisted——only failure statuses (backoff/dead letter/exempt) are stored in the `job_state` table.

## JobKind enum

18 kinds of tasks: local CPU work (extract_tokens, score_candidates, auto_select, sync_oos_queue, sync_aux_liveness, recompute_readiness, bind_distractors, build_plan, gc_media) and external fetching/generation (fetch_definitions, fetch_examples, fetch_etymology, fetch_images, segment_morphology, score_image_clip, gen_image_sdxl, gen_image_codex, rewrite_definition, synth_tts).

### default_rate_key() → RateKey
Which items channel this kind of task runs on by default. Local CPU work and some fetching go to the cpu channel; image fetching defaults to unsplash; TTS goes to edge_tts; and so on. Multi-source tasks (definition, image) override the channel by source when processing rows/lines.

### dead_after_attempts() → number
How many consecutive failures before being judged a dead letter. Generation types (SDXL/Codex) 3 attempts, CLIP score and LLM rewriting 4 attempts, TTS 5 attempts, HTTP fetching 8 attempts, local work 5 attempts.

## RateKey enum

14 scheduling channels: cpu, freedict, wiktionary, unsplash, pexels, pixabay, wikimedia, openverse, tatoeba, sdxl, clip, codex, edge_tts, llm. These correspond one-to-one with the seed rows/lines in the `rate_limits` table in the data database.

## SubjectType enum

4 task subject types: word, def_candidate, tts_input, global.

## JobStatus enum

3 persisted statuses: backoff (in backoff), dead (dead letter), waived (already exempted).

## Priority enum

4 priority bands, lower numbers execute first: P0 (local computation, unlocks subsequent steps), P1 (edit-triggered regeneration; blocks content pending publication), P2 (backlog backfill), P3 (high-cost generation fallback).

## SubjectRef

Identifies a task's subject.

### word(word_id) → SubjectRef
Uses word id as the subject.

### word_source(word_id, source) → SubjectRef
Sub-tasks of the same word fanned out by source, with subject_id formatted as `"word_id:source"`——so one graph database dying won't affect the other two.

### def_candidate(id) / tts_input(hash) / global(name)
Constructors for other subject types.

### word_id() → optional number
Extracts the numeric word id from subject_id.

### source() → optional string
Extracts the source suffix from the `word_id:source` format.

## JobKey

The task's primary key: `(kind, subject)`. Also used as the deduplication key during running/processing. Displays as `"extract_tokens/def_candidate:42"`.

## JobView / LaneView / JobsSnapshot

Data structures returned by `GET /api/jobs`. JobView is a single task view, LaneView is a channel count (queued/running/limit), and JobsSnapshot contains the running list, backoff list, and channel overview.

## Constraints
- RateKey members must correspond one-to-one with the `rate_limits` seed rows/lines in `working-db.sql`; otherwise new channels won't have rate limit configuration.
- The dead_after_attempts threshold directly affects operations——lowering it can cause normal transient failures to be misjudged as dead letters.
- The task queue is not persisted; after a restart it is re-derived from the current data database status. Don't try to serialize the queue itself.


--- lib.rs ---

# domain package entry point

A shared vocabulary package of pure types and pure functions. No I/O, no database dependencies—all content can be unit-tested directly. Hashing rules are the foundation of the entire expiration model, so they live here.

## Export groups

**Text processing**: canonicalize, fold_lemma (from canon); locate (from sentence)

**Hashing**: HashInput, text_hash, file_hash, hash_fields, def_extraction_input_hash, tts_input_hash (from hash)

**Blocking & readiness**: BlockerCode, BlockerSet, parse_blockers (from blocker)

**Change notification**: ChangeEvent, ChangeSet, EntityType (from change)

**Error categorization**: ErrorKind, TaskError (from error)

**Audit logging**: Actor, Action, EventDraft, EventRecord (from event)

**Task scheduling**: JobKind, JobKey, JobStatus, JobView, JobsSnapshot, LaneView, Priority, RateKey, SubjectRef, SubjectType (from job)

**Time**: format_ts, now_ts, parse_ts (from time)

**TTS configuration**: TtsConfig, DesiredTts (from tts)

**Database enums and data structures**: Role, AuxStatus, CreatedBy, SelectedBy, CandidateStatus, CandidateKind, DefinitionSource, ExampleSource, ImageSource, MediaKind, TtsKind, OosStatus, GlossSource, EtymologySource, Pos, ParseEnumError, SlotRef, Word, WordImport, DefinitionCandidate, DefinitionSelection, ExtractedToken, FetchedDefinition, FetchedExample, FetchedImage (from types)

**Version**: SCHEMA_USER_VERSION (from version)


--- sentence.rs ---

# 定位句子中的词

在已规范化（canonicalize）的句子中查找目标词的字节偏移范围，用于示例高亮（`example_candidates.hl_start` / `hl_end`）。

## locate(text, lemma) → optional（起始偏移，结束偏移）

在已规范化的文本中查找词元的位置，返回 UTF-8 字节偏移。查找不区分大小写。

优先匹配原形：如果句子里同时出现 "adapted" 和 "adapt"，高亮原形 "adapt"。原形找不到时，依次尝试常见英语屈折后缀（s、es、ed、d、ing、en、er、ies、ied、ees），以及去尾 e 后加后缀的形式（serene → serener）。

仅在词边界上匹配——"ample" 不会匹配 "example" 里的子串。

空词元直接返回空。

## 约束条件
- 输入文本必须是已经过 canonicalize 的——偏移量是在规范化后的字符串上测量的，传入原始文本会导致偏移错位（misalignment）。
- 返回的偏移量是字节偏移，不是字符偏移，直接用于切片。


--- time.rs ---

# Timestamp

UTC ISO-8601 timestamp tool. The output format is byte-for-byte identical to SQLite's `strftime('%Y-%m-%dT%H:%M:%fZ','now')`.

## format_ts(ts) → string
Formats UTC time into the contract format, e.g., `"2026-08-26T01:02:03.456Z"`.

## now_ts() → string
Returns the current time directly in the contract format.

## parse_ts(raw) → optional timestamp
Parses a timestamp string. First tries RFC 3339, then loosely tries variants without the fractional part or without the trailing Z (compatible with manually edited rows/lines). Returns empty if parsing fails.

## constraint
- The output format must remain byte-identical to SQLite's strftime; otherwise, queries that use string comparison for time sorting will go wrong.


--- tts.rs ---

# TTS Voice Configuration

Configuration and content addressing for speech synthesis. TTS is addressed by "what was synthesized" — changing voice or prosody parameters produces a new hash, and old rows/lines are handed to GC once they lose their references.

## TtsConfig

Reads voice and prosody settings from `morphod.toml [tts]`. Fields: voice (voice ID), rate (speech rate), pitch (pitch), volume (volume), word_bitrate_kbps (word audio bitrate, default 48), text_bitrate_kbps (definition and example audio bitrate, default 32), engine (engine name), engine_ver (engine version).

### bitrate_kbps(kind) → number

Returns the corresponding bitrate based on TTS type (Word/Definition/Example). Word audio uses a higher bitrate because dictation quizzes require clear phonemes.

### params_json(kind) → string

Generates the value for the `tts_assets.params_json` column. The field order is fixed (not dependent on map iteration) because this string participates in hashing — the same configuration with a different ordering must not produce two rows/lines.

### input_hash(kind, text) → string

Content address for a single synthesis. Internally calls tts_input_hash, mixing the text, voice, engine, version, and parameters all together.

### desired(kind, text) → DesiredTts

Generates a complete desired synthesis description for an item, containing hash, text, parameters, and bitrate.

## DesiredTts

A desired TTS synthesis for an item, containing: kind, input_hash, text, params_json, bitrate_kbps. Used to diff the desired set against existing assets.

## Constraint

- The field order of params_json cannot be changed — it participates in hashing; changing the order will make the same configuration produce different asset rows/lines.
- engine_ver is intentionally a configured value rather than an adapter-reported value; changing the version number will cause all existing TTS assets to expire.


--- types.rs ---

# Database Enums and Data Structures

All types corresponding to CHECK constraints in the working database are enums, as are the data structures of the main entities. Each enum serializes to a string that is exactly the value accepted by the SQL CHECK constraint—so values cannot violate the schema through the type system.

## Enum list

All enums support `as_str` / `Display` / `FromStr` / serialization; unrecognized strings return `ParseEnumError`.

- **Role** — `words.role`: `target`, `base`, `auxiliary`
- **AuxStatus** — `words.aux_status`: `active`, `retired`
- **CreatedBy** — `words.created_by`: `import`, `promotion`, `manual`
- **SelectedBy** — `selections.selected_by`: `auto`, `human`
- **CandidateStatus** — `candidates.status`: `available`, `rejected`
- **CandidateKind** — the candidate/selection family an operation addresses: `definition`, `example`, `image`
- **DefinitionSource** — definition source: `freedict`, `wordnet`, `llm_rewrite`, `manual`
- **ExampleSource** — example source: `exam_corpus`, `freedict`, `tatoeba`, `llm`, `manual`
- **ImageSource** — image source: `unsplash`, `pexels`, `pixabay`, `wikimedia`, `openverse`, `sdxl`, `codex`, `manual`
- **MediaKind** — media type: `image` (webp), `audio` (ogg)
- **TtsKind** — TTS kind: `word`, `definition`, `example`
- **OosStatus** — OOS queue status: `open`, `resolved_rewrite`, `resolved_promote`, `resolved_gloss`, `auto_closed`
- **GlossSource** — Chinese definition source: `manual`, `cedict`
- **EtymologySource** — word-origin source: `wiktionary`, `morfessor`, `manual`
- **Pos** — part of speech: `noun`, `verb`, `adj`, `adv`, `prep`, `conj`, `interj`, `phrase`

### Pos.normalize(raw) → Pos

Normalizes part-of-speech tags from external sources into the contracted vocabulary. Closed-class parts of speech (pronouns, determiners, articles, etc.) are folded into `phrase`. Normalization is lossy but honest—the original tag is preserved in the candidate's `source_ref`.

### MediaKind.extension() → string

Returns the file extension used by content-addressed storage: `image` → `"webp"`, `audio` → `"ogg"`.

### MediaKind.content_type() → string

Returns the MIME type served by `GET /api/media/{hash}`.

## ParseEnumError

Error returned when a database/API string cannot match a known variant; contains the enum kind name and the actual value.

## SlotRef

Locates a selection slot: Definition (by `word_id` + `pos`), Example (by `word_id` + slot 1–3), Image (by `word_id`). Provides `word_id()`, `candidate_kind()`, `entity_id()`, and `Display`.

## Word

Read structure for word rows/lines. The `is_active()` method provides the semantics of the `active_words` view: `target` is always active; `auxiliary` is active only when `aux_status` is `active`; `base` is never active.

## WordImport

A row/line from a word-list import. Fields: `word` (alias lemma), `phonetic` (optional), `frequency_rank` (optional).

## DefinitionCandidate / DefinitionSelection

Complete mirrors of definition candidate and selection rows/lines.

## ExtractedToken

A word token from a cached definition extraction: `position`, `surface`, `lemma`.

## FetchedDefinition / FetchedExample / FetchedImage

Candidates fetched from external sources but not yet written to the database. FetchedExample's highlight offsets are UTF-8 byte offsets measured on the normalized text.

## Constraints

- The spelling of enum values must be fully consistent with the SQL CHECK constraints and the admin-ui frontend.
- `Pos.normalize`'s fallback is `phrase`; don't let unknown part-of-speech tags become errors—they should silently fall back to it.
- FetchedExample's `hl_start`/`hl_end` must be measured after canonicalization; passing offsets from the raw text will cut the wrong bytes.


--- version.rs ---

# Algorithm Version Constants

These constants are mixed into the version numbers of derived hashes. Bumping a constant causes all derivatives produced by that tool to expire and be recomputed.

## Constant List

- **CANON_ALGO_VER** (`"canon/1"`) — canonicalizer version; affects all text hashes
- **EXTRACTION_ALGO_VER** (`"def-extract/1"`) — version of the definition extraction input hash
- **TTS_ALGO_VER** (`"tts-input/1"`) — version of the TTS input hash
- **SCORER_ALGO_VER** (`"scorer/5"`) — candidate scorer version; bumping it rescore all candidates
- **CLIP_ALGO_VER** (`"clip/1"`) — CLIP image-text scoring version
- **PLAN_ALGO_VER** (`"plan/1"`) — plan builder version
- **PLAN_INPUT_ALGO_VER** (`"plan-input/1"`) — plan input hash version
- **DISTRACTOR_ALGO_VER** (`"distractor/2"`) — distractor binder version (provenance only; does not trigger expiry)
- **READINESS_ALGO_VER** (`"readiness/1"`) — ready/blocking evaluator version
- **EXPORT_ALGO_VER** (`"export/1"`) — exporter version; incorporated into `releases.input_hash`
- **RELEASE_SCHEMA_VER** (`"2"`) — `release.db` schema version
- **SCHEMA_USER_VERSION** (`7`) — working database's `PRAGMA user_version`; incremented on every schema migration
- **CONTRACT_SCHEMA_VERSION** (`7`) — schema version described in the contract file; cannot exceed `SCHEMA_USER_VERSION`

## Constraints

- Never reuse an old version number to indicate different behavior — reuse would cause old derivatives that should have expired to be treated as valid.
- `CONTRACT_SCHEMA_VERSION` cannot exceed `SCHEMA_USER_VERSION`; `store::schema` has a compile-time assertion.
- Changing `SCORER_ALGO_VER` triggers a full database rescore — confirm that is the effect you want before changing it.
