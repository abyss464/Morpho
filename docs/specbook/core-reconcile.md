# core/reconcile

48 specs.


============================================================
core/crates/reconcile/src/
============================================================


--- backoff.rs ---

# retrybackoff

Calculates the waiting time for the next retry of a failed task. Formula: base delay of 30 seconds × 2^failure count, capped at 1 hour, plus ±20% jitter.

The jitter is derived from the task key's hash value (not a random number), so the same task at the same retry attempt always gets the same delay—making it convenient for testing and post-hoc debugging, while remaining well dispersed across different tasks.

## retry_delay(key, attempts) → Duration

Given a task key and the current failure count, returns how long to wait before the next retry. Grows exponentially, capped at 1 hour. Jitter varies within ±20%.

## constants

- BASE_DELAY = 30 seconds
- MAX_DELAY = 1 hour

## constraints

- Don't use random numbers for jitter—determinism is a design choice.
- Don't create other backoff logic outside this file; the dispatch module is the only consumer of this code.


--- config.rs ---

# Engine Configuration

Declares three configuration blocks: external data sources, image behavior, and subprocess adapters. Core principle: **not configured ≡ exempt** — a data source that lacks credentials or a path isn't "temporarily broken", it simply "doesn't exist", and the fallback chain skips it immediately without wasting retries. Credential-free sources (Free Dictionary, Wiktionary, Wikimedia Commons, Openverse, Tatoeba) are never disabled; they're only classified as reachable or unreachable.

## SourcesConfig
The engine needs to know all external endpoints and credentials.

### Fields
- freedict_url / wiktionary_url / wikimedia_url / wikipedia_url / openverse_url / tatoeba_url: API addresses for each credential-free data source, all with default values
- wordnet_dir: WordNet data directory. Empty = WordNet stage disabled
- corpus_path: Exam corpus JSONL. Empty = this example source disabled
- unsplash_access_key / pexels_api_key / pixabay_api_key: Paid image database keys. Empty = corresponding image database not enabled
- comfyui_url: Local ComfyUI address. Empty = SDXL disabled
- codex_bin: Codex generator binary name
- clip_url: CLIP score sidecar address. Empty = ranking by quality prior only
- user_agent: User-Agent for HTTP requests (Wikimedia requires a real one)
- http_timeout_secs: Request timeout in seconds

### apply_env()
Populates unset fields from environment variables. Keys are read from the environment first (they shouldn't be written into the config file).

### image_key(source) → API key for a given image database; blank counts as unconfigured
### enabled_image_sources() → Lists all usable image sources by priority
### wordnet_dir() → Returns the directory only after validating that data.noun exists
### corpus_path() → Returns the path only after validating that the file exists
### comfyui_url() / clip_url() / codex_bin() → Return after trimming whitespace
### http_timeout() → Clamped to 1–300 seconds
### describe() → One status row/line per data source; used for startup logs and morphod status

## ImagesConfig
Controls behavior at the end of the image chain.

### Fields
- scene_mode: Whether to generate a scene image from the slot-1 example (default: off)
- scene_prompt_ver: Scene prompt template version
- sdxl_steps / sdxl_cfg / sdxl_workflow: Generation parameters (defaults tuned for SDXL-Turbo)
- clip_model: CLIP model identifier
- codex_enabled: Whether to hand words with poor CLIP scores to the codex generator (default: off)
- codex_prompt_ver / codex_threshold / codex_batch: codex prompt version, trigger threshold, per-round batch limit

### apply_env()
Overrides boolean switches and numeric parameters from environment variables. Unknown values trigger a warning but no override.

### clip_model_ver() → Storage key composed of algorithm version + model name
### scene_mark() / codex_mark() → Mark strings used by source_fetch.source, containing version numbers

## ImageProvider
An image provider: source identifier, rate-limit key, whether a key is required, download interval.

### for_source(source) → Looks up the static table
### download_spacing() → Download interval as a Duration

## IMAGE_PROVIDERS
Static array of all image providers, ordered by priority: paid image databases first, open collections last. Open collections have a download interval to avoid 429s.

## ImageSecondPass / IMAGE_SECOND_PASSES
Second-pass search: re-searches the same provider with looser item criteria. Each pass has its own independent completion mark and never overrides records from the strict search. Only applied to open collections — re-searching paid image databases would waste quota.

## AdapterConfig
Call method for Python subprocess adapters.

### Fields
- adapters_root: Working directory for adapter commands
- command: Command template; `{adapter}` is replaced with the adapter name
- morfessor_batch / morfessor_batch_max_age_secs: Word-segmentation batch size and timeout

### command(adapter) → (program, args) Command line after template substitution
### runner() → Launcher program name
### project_dir(adapter) → Project directory parsed from the command template
### root() → Working directory

## ADAPTERS
All subprocess adapter names and descriptions of the impact when missing.

## Constraints
- Don't fill placeholder content for missing data sources — no means no.
- A blank key counts as "unconfigured" — don't pass it out as a valid key.
- A second-pass search's mark must never share a name with any strict-search mark.
- Don't assume version strings contain no special characters — the mark method replaces all non-alphanumeric characters with underscores.


--- dispatch.rs ---

# Task Dispatcher

Receives a batch of derived task specs, claims each one, enqueues to the corresponding lane, executes rows/lines, and records results. Each task type corresponds to one executor.

## Dispatcher

### new(store, registry, executors) → Arc<Dispatcher>
Creates a dispatcher, takes the executor list, and indexes it by task type.

### handles(kind) → bool
Returns whether this dispatcher has an executor for the corresponding type.

### dispatch(jobs) → usize
Claims and asynchronously executes all tasks. Already in-flight tasks are automatically skipped (via registry deduplication). Task types without an executor are silently discarded—the rule is a stub and will be re-derived in the next round. Returns the number actually dispatched.

### drain()
Waits until all in-flight tasks complete. Used for tests and shutdown.

## Execution flow
Each task passes through: queue for lane → check if lane is frozen → wait for rate limit token → acquire concurrency permit → execute rows/lines → record result → release.

## Result classification
- success: clear job_state rows/lines (if any).
- rate limit (RateLimited): freeze the entire lane for a given time, not counted in retry count, automatically re-derived next round.
- other failures: failure count +1; retryable tasks enter backoff, non-retryable or exceeding retry count are marked as dead letter.

## Constraints
- Don't bypass this dispatcher to directly execute rows/lines tasks—lane rate limiting and deduplication all live here.
- Don't implement retry logic inside executors—backoff and dead letter policies are managed by the dispatcher.


--- distance.rs ---

# 编辑距离

Calculates the Damerau-Levenshtein edit distance between two strings, used for selecting confusable distractors (like adapt / adopt / adept).

The variant used is the unrestricted (with alphabet) version, not the approximate "optimal string alignment" version. The difference lies in the scoring of consecutive transposition chains — the approximate version overcounts.

## damerau_levenshtein(a, b) → usize

The edit distance between two strings, allowing insertion, deletion, substitution, and transposition of adjacent characters. Counted by Unicode characters (multibyte characters count as one). Symmetric.

## lemma_distance(a, b) → usize

Convert to lowercase first, then compute the distance. worditems comparisons use this.

## constraint

Don't replace with the "optimal string alignment" approximation — confusable word selection is exactly the scenario where it computes incorrectly.


--- engine.rs ---

# Reconciliation Loop Engine

Level-triggered reconciliation loop. Three wake sources: startup (one full-pass derive, guaranteeing convergence after any crash/deployment/offline edit), timer (full pass every 60 seconds), and change events (partial, after a 250ms coalesce window).

One pass = inline maintenance scan (score → selection → OOV → liveness → interference → plan → readiness → GC) + derive external tasks from snapshot + hand off to dispatcher.

## EngineContext
Everything rules and executors need to access the external world: data source collection, media storage, TTS config, plan parameters, text pipeline, image config.

### new(sources, media) → EngineContext
Creates with default values. Other fields are set via builder methods.

### with_images / with_tts / with_pipeline / with_plan_params
Chained builders, each replaces the corresponding field. with_pipeline's tokenization/word-form restoration must be consistent with the exporter, otherwise the two sides' judgment of "stale" would differ.

## ReconcilerConfig
Loop parameters: full_pass_interval (default 60 seconds), coalesce_window (default 250 milliseconds).

## PassStats
What one pass did: how many tasks were derived, how many dispatched, how many skipped (in-flight rows/lines/backoff/dead letter/exempt), maintenance scan statistics.

## Trigger
Wake reason: Startup / Interval / Change.

## Reconciler

### new(store, context, config) → Reconciler
Builds the complete engine, registering all rules and executors.

### with_parts(store, context, config, rules, executors) → Reconciler
For testing, manually injects rules and executors.

### registry() / dispatcher() / context()
Gets internal handles. registry is used by GET /api/jobs.

### run(shutdown)
Main loop. Listens for the close signal, timer, and change events, until shutdown is true. On start, first does one full pass, then enters the select loop. Change events are coalesced before partial derive.

### run_once(scope) → Result<PassStats>
One pass of maintenance + derive + dispatch. Publicly exposed for tests to call directly. Internally, it first runs the maintenance scan, then in a read-only transaction loads Facts, calls all rules' derive, then filters by job_state (dead letter, backoff, in-flight, exempt), sorts, and hands off to the dispatcher.

## Constraints
- Don't assume a partial pass can replace the full pass — the full pass is always a superset of the partial one; dropped events wait at most one more minute.
- A rule's derive failure cannot prevent other rules from running — each rule catches independently.
- Maintenance scan failure cannot prevent derive — scan results are recorded as zero, and the loop continues.


--- facts.rs ---

# Per-Round Facts Snapshot

The joined query results that all rules need (which words have definitions, which data sources have been tried, which jobs died...), loaded once per round and handed to all rules as a shared immutable view. The entire set is read in a single read-only transaction, so all rules see the same consistent snapshot.

The rationale is to guarantee that deriving the full volume for 6000 words is a fixed-count index scan, rather than one query per word per rule.

## Facts

### load(conn, clip_model_ver) → Result<Facts>
Loads all facts from a read-only connection. clip_model_ver determines which model's semantic scores to load — cosine values from different models are not comparable, so only the current model's rows are loaded.

### Main fields
- active: active word list, sorted by word frequency
- definitions_fetched / examples_fetched / etymology_fetched / images_fetched: completion flag for each fetch type
- jobs: persisted job failure status
- words_with_definitions / words_with_examples / words_with_images: sets of words that have usable candidates
- words_with_distinct_images: has a usable image and that image hasn't already been selected for another word (after deduplication)
- words_with_library_images: same as above, but only counting image database sources, not generated ones
- scene_image_vers: set of SDXL scene image versions per word
- slot_one_example: slot-1 example text per word
- primary_gloss / primary_gloss_tokens / primary_pos: primary gloss text, content word items, part of speech
- image_candidates / selected_image: image candidate list and current selection
- question_mates: word graph for shared answer cards (symmetric)
- clip_query / clip_scores: CLIP query text and already-available similarity scores
- tts_desired / tts_assets: desired TTS and already-available TTS assets

### Query methods
- job_status(key) → a job's status
- needs_image_candidates(word_id) → whether image candidates are still needed
- needs_library_image(word_id) → whether no usable image was found in the entire image database
- clip_score(word_id, file_hash) → the cosine value of an image for this word under the current model
- best_clip_score(word_id) → the highest CLIP score among all candidates
- unscored_images(word_id) → list of image hashes not yet scored
- mate_image_hashes(word_id) → set of image hashes already selected by answer-card mates
- has_scene_image(word_id, prompt_ver) → whether a scene image of that version already exists
- fetched(markers, word_id, source) → whether a data source has already been fully fetched
- source_exhausted(markers, job_kind, word_id, source) → whether a data source is already exhausted (zero results, or the job died/was exempted)
- exhausted_at(...) → the exhaustion timestamp
- missing_tts(config) → which TTS assets are still missing

## FetchKind
Fetch type enum: Definitions / Examples / Etymology / Images.

## Data source name constants
SOURCE_FREEDICT / SOURCE_WORDNET / SOURCE_WIKTIONARY / SOURCE_MORFESSOR / SOURCE_EXAM_CORPUS / SOURCE_TATOEBA, plus three mapping functions definition_source_name / example_source_name / image_source_name.

## Constraints
- Don't query the database directly in rules — use the fields in Facts. Facts is loaded once and shared by all rules.
- source_fetch.source is a free-text column, and alignment across candidate tables relies entirely on consistent spelling — the name mapping functions are the single source of truth.
- Don't mix scores from different CLIP models in the same comparison.


--- graph.rs ---

# Learning plan graph

Pure, deterministic algorithm that builds the learning plan from definition dependencies. Tarjan SCC on the dependency graph, Kahn's canonical topological sort on the condensation, then grouping into 15-20 word packs. An edge `a -> b` means "a's selected definition uses b", so b is placed before a. Cycles form SCC groups that are learned together. The same DB state produces byte-identical output: nodes are pre-sorted by `(frequency_rank, word_id)` and all tie-breaking is pinned.

## build_plan(input) -> BuiltPlan

Takes a `PlanInput` (nodes, edges, params) and returns the full plan: ordered groups with type (scc/root/semantic/fill), learning_order for each word, and statistics.

## PlanParams

Group sizing knobs: `group_min` (default 15), `group_max` (default 20).

## Constraints
- Pure and total: no I/O, no randomness, no mutable global state.
- Do not change the sort key or Tarjan visit order without understanding that it changes every plan hash.


--- lexicon.rs ---

# Lexicon cache

Immutable snapshot of every `words.lemma`, case-folded via `fold_lemma`. Used by the morphy lemmatizer and the OOV scanner to decide whether a definition token matches a known word.

## Lexicon::load(conn) -> Lexicon

Reads all lemmas from the `words` table, folded.

## Lexicon::contains(folded) -> bool

Membership test against the snapshot.

## LexiconCache

Arc<RwLock> wrapper refreshed once per reconcile pass before any stage reads it. Deliberately not mixed into `def_extractions.input_hash` -- folding a whole-table fingerprint would re-run every extraction on every word import.


--- lib.rs ---

# Reconcile crate root

Re-exports the public API of the reconciliation engine. Submodules: `rule`/`rules` (desired-state derivation), `exec` (one executor per job kind), `stages` (inline local sweep), `sources` (HTTP, WordNet, corpus, Python adapters), `graph`/`score`/`distance`/`readiness` (pure algorithms), `registry` (job dispatch lanes), `engine` (the reconciler loop), `lexicon`/`morphy`/`text` (tokenization and lemmatization).


--- morphy.rs ---

# Morphy lemmatizer

WordNet-style morphological analyzer validated against the Morpho lexicon. Strips inflectional suffixes and checks that the result is a known word; unknown detachments are discarded so out-of-scope tokens stay honestly unresolved. No part-of-speech input -- all rules are tried, longest suffix first, and lexicon validation prevents nonsense.

Resolution order: (1) surface itself if it is a lemma, (2) exception table, (3) detachment rules longest-suffix-first, (4) consonant-doubling reversal for -ing/-ed, (5) surface unchanged.

## MorphyLemmatizer::new(lexicon, exceptions?) -> MorphyLemmatizer

Builds the lemmatizer with compiled-in irregular forms, optionally extended with WNdb exception files from a directory.

## MorphyLemmatizer::lemmatize(surface) -> &str

Returns the best lemma for a surface form.

## Constraints
- Protected surfaces (number, her, etc.) are never detached even when the result is a valid word.
- Version strings (`MORPHY_LEMMATIZER_VER` / `MORPHY_LEMMATIZER_WNDB_VER`) are written to `def_extractions.lemmatizer_ver`; changing rules requires bumping the version.


--- readiness.rs ---

# Readiness evaluation

Pure fold over `WordFacts` that computes `core_ready` and `ready` for every word.

`core_ready(W)` = primary sense selected and approved, every enabled sense approved, no OOS pending, dependencies covered, example slot 1 approved, live image approved, all TTS ready, word in plan.

`ready(W)` = `core_ready(W)` + three distractors bound + each distractor core_ready.

The split caps recursion at depth 1: mutual distractors (adapt/adopt) cannot deadlock because a distractor only needs core assets, never its own distractors.

## core_blockers(facts) -> BlockerSet

Computes the non-distractor portion of the blocker set.

## evaluate_all(all_facts) -> Vec<Readiness>

Evaluates every word in one pass, using core_ready results to resolve distractor readiness.

## Constraints
- Pure module: facts come from a single snapshot read, evaluation is a fold.
- Do not add distractor-depends-on-distractor logic; depth-1 capping is a design invariant.


--- registry.rs ---

# Job registry and dispatch lanes

In-memory job tracking for the reconciler. QUEUED/RUNNING states exist only here; losing them to a crash costs nothing because the startup full pass rediscovers every unmet need.

## Lane

One dispatcher swim lane: a token-bucket rate limiter plus a concurrency semaphore. Configurable via `rate_limits` rows in the DB (rate_key, max_concurrency, refill_per_min, burst). Falls back to `FALLBACK_LIMIT` (2 concurrent, 60/min, burst 5) when no row exists.

## JobRegistry

Tracks in-flight and backoff jobs, manages lane assignment, exposes `JobsSnapshot` for the admin API's `GET /jobs` endpoint.


--- rule.rs ---

# Rule interface

A rule is a pure function from a read snapshot to the set of jobs needed to reach desired state. Rules never write, never deduplicate against in-flight work, and never consult backoff state. Exports `Rule` trait, `JobSpec` (what a rule emits), `JobPayload` (typed payloads per job kind), `Scope` (Full vs Partial pass), and `Snapshot` (the read view rules operate on). Rules derive external work only; all local stages run inline in `stages/`.


--- score.rs ---

# Candidate scoring

Pure scoring functions for definition, example, and image candidates. Inputs: readability against the lexicon, OOS token penalty, length window, source priors, sense frequency, POS/primary-sense match, and resolution for images. Scores are in `[0, 1]` with breakdown stored in `score_detail`. A `HYSTERESIS_DELTA` of 0.05 prevents two closely-scored candidates from trading the slot on every pass. `scorer_ver` bump is the only thing that invalidates scores.

## Key functions
- `score_definition(...)` -- readability + length + source + frequency
- `score_example(...)` -- readability + length + source
- `score_image(...)` -- resolution + primary-sense match + strategy penalty
- `ImageStrategy` -- stock / sdxl / scene, with configurable fallback order


--- text.rs ---

# Tokenization and lemmatization traits

Defines `Tokenizer` and `Lemmatizer` traits behind which the morphy implementation sits. Both are versioned; the version feeds `def_extractions.input_hash`. `SimpleTokenizer` strips possessives, drops abbreviations from a compiled allowlist, and splits on word boundaries. `TextPipeline` composes a tokenizer and lemmatizer into the extraction pipeline. `ABBREVIATIONS` lists tokens a Chinese junior-high graduate reads without help (e.g., etc., i.e.).


============================================================
core/crates/reconcile/src/exec/
============================================================


--- definitions.rs ---

# Definition Fetch Executor

Gets word definitions from Free Dictionary API or WordNet, writes to the working database.

## FetchDefinitionsExecutor

### new(context) → Self
Constructed with engine context. The context contains HTTP client, source configuration, WordNet instance, etc.

### run(job, store) → Result
Runs a single definition fetch task (rows/lines). Determines which path to take based on the `source` field in the payload:

- **Freedict**: Calls the Free Dictionary API. On success, fetches three artifacts: definition, pronunciation, and example. A 404 is not a failure—"this dictionary doesn't have this word" is itself an answer; write a zero-result completemark. When fetching the example, the definition and example are committed together in a single transaction (two completemarks, one write), so the example is a free byproduct.
- **WordNet**: Queries local WordNet data. Each word takes at most 4 senses (more senses are too obscure, useless for learners). WordNet does not produce examples. If WordNet is not configured, directly return a permanent error.

Finally, call an internal commit function to write the definition (and optionally the example) to the store in a single atomic write.

## Constraint
- Don't assume Free Dictionary's 404 is a fault that needs retry—it is a legitimate "no result" answer; write a completemark so the rule stops repeated derivation.
- The definition and example must be written to the database in the same transaction; they cannot be written separately. Otherwise, after a crash, there will be an inconsistent state where only the definition mark exists, without the example mark.


--- etymology.rs ---

# Word Etymology Fetching and Morphology Segmentation Executors

Two executors: fetch word etymology from Wiktionary, and use Morfessor for morphological segmentation (as a fallback).

## FetchEtymologyExecutor

### new(context) → Self
Constructed with the engine context.

### run(job, store) → Result
Query Wiktionary to get a word's etymology. Wiktionary not having the word (404) is not a failure; record a zero-result completion mark so the Morfessor fallback rule knows it's time to step in.

Write operations are batched in one commit: the completion mark and (if present) the etymology data are submitted together.

## SegmentMorphologyExecutor

### new(context) → Self
Constructed with the engine context.

### run(job, store) → Result
Takes a batch of words and sends them to the Morfessor adapter for morphological segmentation. The payload is a set of (word_id, lemma) pairs.

For each word:
- If the adapter returns more than one morpheme, join them with " + " and write to the etymology field.
- If only one morpheme is returned, the word has no internal structure; record it as a zero result (don't write to the etymology column, but do write the completion mark).
- All results are committed in a single batch transaction.

An empty list directly returns success without writing anything.

## Constraints
- Wiktionary's "no word items" must be marked complete, otherwise the Morfessor fallback will never trigger.
- Morfessor's single-morpheme results must not be written to the etymology column—they indicate "no structure," not etymology information.


--- examples.rs ---

# Example Fetch Executor

Gets a word's examples from the exam corpus database, Free Dictionary, or Tatoeba.

## FetchExamplesExecutor

### new(context) → Self
Constructed with the engine context.

### run(job, store) → Result
Selects the source based on the `source` field in the payload:

- **ExamCorpus**: Reads from the locally loaded corpus database at startup, performing no network I/O. If the corpus database is not configured, returns a permanent error.
- **Freedict**: Backfill path — only used for old words that were already in the database when definition fetching had not yet mined examples. New words do not go down this path (the definition executor already commits both outputs in a single pass). A 404 is recorded as an empty result.
- **Tatoeba**: Calls the Tatoeba API to search for examples. A 404 is recorded as an empty result.

All highlight offsets are computed based on the normalized sentence text (handled uniformly by the shared sentence module): they are computed at load time for the corpus database, and at parse time for the two HTTP sources.

### commit(store, word_id, source, examples) → Result
Commits all examples from a single fetch together with the completion mark in one atomic write operation. Used by FetchExamplesExecutor itself, and also called by other modules within the crate (`pub(crate)` visibility).

## Constraints
- Regardless of how many items the source returns (including zero items), the completion mark must be written. Without the mark, the rule will re-derive this task indefinitely.
- Do not run new words through the Freedict backfill path if their examples have already been mined by the definition executor — that only wastes traffic.


--- extract_tokens.rs ---

# Word Token Extraction Executor

For an itemsdefinition candidate, perform word segmentation, take the results along with the input hash, and store them in the database.

## ExtractTokensExecutor

### new(pipeline) → Self
Constructed using the text processing pipeline. The pipeline contains a word segmenter and a word form lemmatizer.

### run(job, store) → Result
Take the definition candidate's ID, original text, and text hash from the payload. Call the pipeline's extract method to perform word segmentation, then use the record_extraction write operation to commit the word token list, input hash (computed from the text hash + word segmenter version + lemmatizer version), and version number together.

If the write operation finds that the candidate has already changed (input drifted), it returns applied: false. This is not an error — the next round of reconciliation will re-derive the task.

## constraint
- Don't bypass pipeline.input_hash and compute the hash yourself — it binds the text hash, word segmenter version, and lemmatizer version together, and is the only proof of version invalidation.


--- images.rs ---

# Image Fetching, Generation, and Scoring Executors

Four executors: search images from the image database, generate images with SDXL, generate images with Codex, and score images with CLIP.

## FetchImagesExecutor

### new(context) → Self
Constructed with engine context.

### run(job, store) → Result
Search for images from the configured image database. Process: search → download images one by one (with provider-required intervals) → transcode to 768x576 WebP → hash and store into the content-addressed media database → register rows/lines in the database.

Write the file first, then write the database rows/lines — a crash only leaves orphaned files (reclaimed by cleanup tasks), and never produces rows/lines pointing to nonexistent files.

There are two search query strategies: strict mode uses word items plus definition content words; broad search mode uses definition keywords after word segmentation (taking the two longest ones). A single image download failure (permanent error) only skips that image without interrupting the whole batch.

## GenImageSdxlExecutor

### new(context) → Self
Constructed with engine context.

### run(job, store) → Result
Call the ComfyUI/SDXL adapter to generate an image. There are two prompt word modes:

- **Scene mode**: uses sentences to describe the scene, with word items indicating the subject and definitions aiding disambiguation. The seed is determined by word_id + template version; the same word and version always produce the same image.
- **Bare concept mode**: constructs the prompt using only word items and definitions. The seed is determined by hashing the prompt text.

The generated result is saved to a staging directory, then hashed into the media database. The negative prompt consistently excludes text, watermarks, logos, etc.

## GenImageCodexExecutor

### new(context) → Self
Constructed with engine context.

### run(job, store) → Result
Call the Codex adapter to generate an image. Pass word items, part of speech, definition, example, and template version to the adapter. After generation, similarly save to the staging directory and hash into the media database. The model name and template version are recorded in source_ref, allowing downstream judgment to distinguish images produced by different templates.

## ScoreImageClipExecutor

### new(context) → Self
Constructed with engine context.

### run(job, store) → Result
Call the CLIP adapter to compute semantic similarity between a set of images and text.

Key validation: the model version returned by the adapter must match the engine configuration; any mismatch directly results in permanent failure — if cosine values from two models are mixed under one identity, downstream cannot distinguish them.

If the adapter reports that some image files cannot be read, only issue a warning without interrupting. But if all images fail to be read (returning zero results), directly fail permanently and move to the dead letter queue — this indicates the media directory is misconfigured, making retries pointless.

## Constraints

- Images must be written to disk before writing database rows/lines; the order absolutely cannot be reversed.
- CLIP model version mismatch must result in permanent failure and cannot be swallowed as a warning.
- SDXL's scene seed can only be determined by word_id and template version; prompt text must not be mixed in — otherwise re-selecting a sentence would change the seed and produce a different image.
- All images must be transcoded to 768x576 WebP before storage; the media database does not accept other formats/sizes.


--- mod.rs ---

# executor module entry point

Declare and export all task executors, define the Executor trait and helper functions.

## Exported Executors

Grouped by responsibility:

- **definition**: FetchDefinitionsExecutor
- **word source**: FetchEtymologyExecutor, SegmentMorphologyExecutor
- **example**: FetchExamplesExecutor
- **token extraction**: ExtractTokensExecutor
- **image**: FetchImagesExecutor, GenImageSdxlExecutor, GenImageCodexExecutor, ScoreImageClipExecutor
- **speech**: SynthTtsExecutor

## Executor trait

Each executor implements this trait:

- **kind() → JobKind**: returns the task type corresponding to this executor.
- **run(job, store) → Result**: executes a task. Each execution produces an atomic write operation — the result and its source marker are written to the database in the same transaction, so there is no window where a result exists without provenance. The executor itself does not hold a database connection; it writes through the store handle.

## default_executors(context) → Vec\<executor\>

Constructs the full set of executors. Called once per process, used at engine startup.

## store_error(err) → TaskError

Converts a store-layer error into a task error. Invalid types are converted to permanent errors; the rest (write queue full, channel closing) are converted to transient errors.

## wrong_payload(kind) → TaskError

Used when the payload and executor do not match. This is a compile-time/registration-time bug, a permanent error, and retry cannot fix it.


--- tts.rs ---

# TTS Synthesis Executor

Call the TTS adapter to synthesize speech, producing an Ogg Opus audio file.

## SynthTtsExecutor

### new(context) → Self
Constructed with the engine context.

### run(job, store) → Result
Extract the text to synthesize, voice parameters, bitrate, etc. from the payload, and call the TTS adapter.

- **success**: Write the audio file to a staging directory first, then hash it into the content-addressed media database, and finally commit the media registration rows/lines and TTS asset rows/lines in a single transaction. TTS asset rows/lines never point to an unregistered file.
- **permanent failure**: Write an items failed asset rows/lines, allowing the console to display the failure reason and letting the expectation set differential stop deriving this task. The error is still propagated upward.
- **transient failure**: Write nothing. Retry is handled by job_state management — writing failed rows/lines at the first network glitch would be a lie, because this synthesis has not truly been abandoned.

engine_ver field: If the adapter returns the engine version actually used for rows/lines, use it; otherwise, use the version from the configuration.

## Constraints

- Transient failures absolutely must not write failed rows/lines — that would cause a recoverable task to stop permanently.
- Media registration and TTS asset rows/lines must be in the same transaction; they cannot be written separately.


============================================================
core/crates/reconcile/src/rules/
============================================================


--- definitions.rs ---

# Definition Fetching Rule

Determines which words need to have definitions fetched, and which source to query first.

## FetchDefinitionsRule

### new(context) → Self

Constructed with the engine context.

### derive(snapshot) → Result\<Vec\<JobSpec\>\>

Iterate over all active words, deriving tasks by the following logic:

1. **Free Dictionary (primary source)**: No API key required. Derive a fetching task for each word that hasn't been looked up yet. Go through the Freedict rate limit channel.
2. **WordNet (fallback)**: Only derive when all three conditions are met — WordNet is already configured, this word still has no definition candidates, and Free Dictionary has already been used up (queried and returned zero results, or the task is already dead/already exempt). Go through the CPU channel (local query).

All tasks have priority P2 (backlog backfill), sorted by word frequency — words the learner encounters first take priority in becoming usable status.

## Constraints

- WordNet is only a fallback, not a "second opinion" — once a word has gotten a definition candidate from any source, WordNet should not be queried again.
- When WordNet is not configured, the fallback path must be completely skipped, and cannot derive a task that is doomed to permanent failure.


--- etymology.rs ---

# Word Source Fetching and Morphology Segmentation Rules

Two item rules: Wiktionary word source lookup (primary source) and Morfessor morphological segmentation (batched fallback).

## FetchEtymologyRule

### new(context) → Self
Constructed with engine context.

### derive(snapshot) → Result\<Vec\<JobSpec\>\>
For each active word that still has no word source and hasn't been queried on Wiktionary yet, derive a FetchEtymology task. It goes to the Wiktionary rate limit channel with priority P2.

## SegmentMorphologyRule

### new(context) → Self
Constructed with engine context.

### derive(snapshot) → Result\<Vec\<JobSpec\>\>
Collect all words that satisfy the item conditions: no word source, Wiktionary already fully used, and Morfessor segmentation not yet done. Then decide whether to emit a batch based on one of two conditions:

- **Batch already full**: the number of pending words reaches the configured batch size.
- **Waited too long**: the earliest waiting word's Wiktionary completion time exceeds the configured maximum wait period.

If neither condition is met, keep accumulating and derive no tasks.

Batch size is capped at 5000 (the adapter's own limit). Words are sorted by word frequency; the same database status always produces the same batch (deterministic), because for Morfessor's temporary training model, the version digest overrides the training words.

Tasks have priority P3 (an expensive generative fallback) and go to the CPU channel. There is only one task body globally (morfessor_batch).

## Constraints
- Morfessor cannot be triggered before Wiktionary is fully used — it is a fallback, not a supplement.
- Don't let per-row/per-line emission bypass the batch size / maximum wait period gating — batches that are too small will let the temporary trainer produce garbage.
- Batch membership must be deterministic (sorted by word frequency + word_id); otherwise, the same status will produce different model version digests.


--- examples.rs ---

# Example Fetching Rule

Determines which words need to get examples from which sources.

## FetchExamplesRule

### new(context) → Self
Constructed with engine context.

### derive(snapshot) → Result\<Vec\<JobSpec\>\>
For each active word, check three sources:

1. **Exam corpus database**: only derive when corpus_path is already configured. Reads local files, goes through CPU channel.
2. **Free Dictionary (backfill)**: only derive when the word's definition has already been fetched from Freedict, but its example hasn't been fetched yet. This is to backfill historical data—older words that entered the database before the definition executor began mining examples. New words won't go through this path (the definition executor already submits examples along with the definition). Goes through Freedict rate limit channel.
3. **Tatoeba**: no prerequisites (no API key required); derive tasks for every word that hasn't been looked up yet. Goes through Tatoeba rate limit channel.

All tasks have priority P2, sorted by word frequency.

## Constraints
- When the corpus database is not configured, don't derive ExamCorpus tasks—that would only produce permanent-error dead letters.
- Free Dictionary backfill only targets old words (definition already fetched, example not fetched). Deriving backfill for new words is wasteful: the definition executor already brings examples back.


--- extract_tokens.rs ---

# word meta extraction rule

Determines which definition candidates need (re-)extraction of word meta.

## ExtractTokensRule

### new(pipeline) → Self
Constructed using the text processing pipeline.

### derive(snapshot) → Result\<Vec\<JobSpec\>\>
Directly queries the data database (not going through the shared fact set), finds all definition candidates with status = available, and checks whether the input_hash of their def_extractions rows/lines equals the expected value computed by the current pipeline (blake3(text_hash + tokenizer_ver + lemmatizer_ver)). If they differ or don't exist, it derives an ExtractTokens task.

This is the only rule where items directly query the database instead of reading the fact set — because it needs the full text of every item's candidates, which is the largest table in the database; in the common case of "nothing to do", loading it all into memory is pointless.

Task priority P0 (cheap local computation, unblocks downstream dependencies), goes to the CPU channel, sorted by word frequency.

## Constraints
- Don't change it to read from the fact set — the memory overhead of all definition candidate texts is not worth bearing for the sake of symmetry.
- The input_hash calculation must use pipeline.input_hash; it is the only method that can correctly reflect invalidation caused by tool version upgrades.


--- images.rs ---

# imageget, generate, and scorerule

Six item rules form an item's complete imageget chain: first-round search → second-round broad search → CLIP score → SDXL generate → scene generate → Codex generate.

## FetchImagesRule

### derive(snapshot) → Result\<Vec\<JobSpec\>\>

For each active word still lacking usable image candidates, derive a strict search task against each already-enabled image database. Priority P2.

## FetchImagesSecondPassRule

### derive(snapshot) → Result\<Vec\<JobSpec\>\>

Second-round search. Trigger condition: the word still lacks image candidates, and all first-round searches are already complete. Progress level by level through the configured IMAGE_SECOND_PASSES list: search Openverse with relaxed license, search Wikimedia with broad keywords, search Openverse with broad keywords. Each word only advances one level per round — the next level is derived only after the previous level has written its mark.

## ScoreImageClipRule

### derive(snapshot) → Result\<Vec\<JobSpec\>\>

For each active word with unscored images (excluding Chinese-definition anchor words), derive a CLIP score task. Each word scores at most MAX_IMAGES_PER_REQUEST images per round; the remainder is scored in the next round. No completion mark — the score rows/lines themselves are the mark. The CLIP adapter must already be configured. Priority P2.

## GenImageSdxlRule

### derive(snapshot) → Result\<Vec\<JobSpec\>\>

Local SDXL generation in bare concept mode. Trigger conditions: the word lacks image candidates, all image databases (including the second round) are already fully used, and SDXL is already configured. If scene mode is enabled and the word has an example, leave it to the scene rule. ComfyUI must be available. Priority P3.

## GenSceneImageRule

### derive(snapshot) → Result\<Vec\<JobSpec\>\>

Local SDXL generation in scene mode. Same level as the bare concept rule, with the same gating conditions, but uses the word's example to construct the prompt. When the word has no example, it falls back to the bare concept rule. Each template version generates once — upgrading the version moves the task body without needing to clear marks. Priority P3.

## GenImageCodexRule

### derive(snapshot) → Result\<Vec\<JobSpec\>\>

The final level: use the external Codex adapter to generate images for words where "none of the existing images are suitable." The trigger conditions are the strictest: codex_enabled is on, the adapter is online, all image databases are fully used, SDXL is also fully used (if configured), and the word's best CLIP score is below the threshold (or there is no score). Words without examples are shelved (not generated individually from the word item). At most codex_batch tasks are derived per round. Priority P3.

## Helper Functions

- **pass_spent(facts, word_id, mark)**: Determines whether an image search stage has ended (has a completion mark, or the task is already dead/exempt).
- **libraries_spent(facts, enabled, word_id)**: Whether all image databases (including the second round) are fully used up. This is the overall gate for the generation layer.
- **generation_spent(facts, word_id, sdxl_name)**: Whether SDXL has already produced results (bare concept or scene, either is acceptable).
- **inapt(facts, word_id, threshold)**: Whether the word's best image CLIP score is below the threshold, or there is no score at all.

## Constraints

- Generation rules (SDXL/Codex) cannot trigger until all image databases are fully used — real photos take priority over generated images.
- The second-round search must proceed level by level and cannot derive everything at once — each word only spends one request per round.
- The Codex rule must be shelved when the word has no example (no generation); once the example arrives next round, it will be derived naturally.
- The CLIP score rule does not use completion marks — the score rows/lines themselves are the completion evidence; an extra mark would require invalidation handling when slot 1 changes.


--- mod.rs ---

# rule module entry point

Declares and exports all expected rules, providing a complete rule set constructor.

## Exported rules

Grouped by responsibility:

- **definition**: FetchDefinitionsRule
- **word source**: FetchEtymologyRule, SegmentMorphologyRule
- **example**: FetchExamplesRule
- **word meta extraction**: ExtractTokensRule
- **image**: FetchImagesRule, FetchImagesSecondPassRule, ScoreImageClipRule, GenImageSdxlRule, GenSceneImageRule, GenImageCodexRule
- **speech**: SynthTtsRule

## default_rules(context) → Vec\<rule\>

Constructs the entire rule set in derivation order. The order is meaningful:

1. ExtractTokens (P0, local computation, unlocks downstream dependencies)
2. FetchDefinitions → FetchExamples → FetchEtymology → SegmentMorphology (P2/P3, network fetching)
3. FetchImages → FetchImagesSecondPass → ScoreImageClip (P2, image fetching and scoring)
4. GenImageSdxl → GenSceneImage → GenImageCodex (P3, generative fallback)
5. SynthTts (P2, speech synthesis)

All fallback chains follow a unified expression pattern: a fallback rule does not derive anything before the primary source has been fully "used up" (queried and empty / dead letter / exempt / unconfigured).


--- tts.rs ---

# TTS Synthesis Rule

Determines which texts need synthesized speech.

## SynthTtsRule

### new(context) → Self
Constructed with engine context.

### derive(snapshot) → Result\<Vec\<JobSpec\>\>
Collects all missing TTS items from the fact set (each active word item, selected word's definition, selected word's example). Combined with the current voice configuration, computes input_hash. Those without a corresponding ready asset are the missing ones. For each missing item, derives one SynthTts task.

The task key is input_hash rather than word_id — when two words' definition texts happen to be identical, they share one synthesis, and one item becomes a dead letter. Sorting uses the first 12 hexadecimal digits of input_hash converted to an integer; there's no word frequency concept, but stability is guaranteed.

Priority P2, goes through the EdgeTts rate limit channel.

## Constraints
- Changing voice configuration doesn't require cleaning up old data — new configuration produces a new input_hash, and old rows/lines are naturally reclaimed by GC. Don't manually delete old TTS asset rows/lines.


============================================================
core/crates/reconcile/src/sources/
============================================================


--- clip.rs ---

# CLIP scoring sidecar client

HTTP client for the CLIP similarity scoring service. The GPU work runs outside the engine on the host (under the ComfyUI venv with ROCm torch + open_clip). Communicates via content hashes, not bytes -- the sidecar resolves `{root}/{hash[:2]}/{hash}.webp` itself. One JSON round trip scores a word's entire candidate pool. Classifies failures as `Permanent | Transient | RateLimited`.


--- corpus.rs ---

# Exam corpus source

Loads example sentences from a JSONL file (`corpus_path`). Each line has `word`, `sentence`, and optional `source` (provenance). Highlight offsets are computed after canonicalization. Sentences where the target word cannot be located are dropped rather than stored with a guessed range. Without `corpus_path`, this source is absent and every word reports `missing_example`.


--- freedict.rs ---

# Free Dictionary API source

Client for `https://api.dictionaryapi.dev`. Extracts definitions grouped by POS and usage examples from the response. 404 maps to `Permanent` and records a completion marker so the word is never re-fetched. Parsing tolerates extra fields since the API adds keys without notice.


--- http.rs ---

Shared HTTP plumbing for network sources. `build_client(config)` creates the process-wide `reqwest::Client` with connection pooling, user agent, and timeout. `classify_response(response)` maps HTTP status codes to the `Permanent | Transient | RateLimited` taxonomy that drives retries. Parses `Retry-After` headers for rate-limited responses.


--- images.rs ---

# Image providers and WebP encoder

Two families of image sources. Keyed stock libraries (Unsplash, Pexels, Pixabay) -- disabled when no API key is configured, which rules treat as waived. Keyless open libraries (Wikimedia Commons, Openverse) -- always enabled, carry per-file licence metadata on the candidate. Wikimedia searches by filename in the File: namespace first, then falls back to the lead image of the word's English Wikipedia article. All fetched images are encoded to WebP before storage.


--- mod.rs ---

# Sources module root

Re-exports and assembles the `SourceSet`: in-process sources (HTTP via reqwest for Free Dictionary, Wiktionary, Wikimedia Commons, Openverse, Tatoeba, stock-photo APIs; WordNet from WNdb files; exam corpus from JSONL) and subprocess adapters (tts, morfessor, sdxl, codex, clip) speaking the adapter protocol. Every source is a pure function from typed input to typed output plus the Permanent/Transient/RateLimited error taxonomy. None sees the database.


--- proc.rs ---

# Adapter subprocess protocol

Implements `docs/contracts/adapter-protocol.md`. One process per job: morphod writes a JSON request to stdin, closes it, reads one JSON response from stdout, and kills the child at the contractual timeout. stderr is captured into `job_state.last_error`. Invocation: `uv run --project adapters/<name> <name>-adapter` from the `adapters_root`. Exit code 2 is a protocol crash; any non-zero exit maps to `Transient`. Also provides `probe_adapters()` for startup health checks.


--- sentence.rs ---

Shared sentence processing for all example sources. Canonicalizes text first (collapsing whitespace), then locates the target word case-insensitively with whole-word matching including regular English inflections. Sentences where the target cannot be found are dropped -- a wrong highlight is worse than a missing example.


--- tatoeba.rs ---

# Tatoeba sentence search

Client for `https://tatoeba.org/en/api_v0/search`. Community corpus of natural sentences, free of credentials. Records per-sentence licence (CC BY 2.0 FR, CC0 1.0, etc.) on each candidate. Results are relevance-ranked, so every sentence is re-checked locally against the lemma; sentences that do not actually contain the word (or a simple inflection) are dropped.


--- wiktionary.rs ---

# Wiktionary etymology source

Fetches etymology from the MediaWiki action API wikitext. Reads the English section's `===Etymology===` subsection and unwraps etymology-carrying templates into readable English; other templates are dropped. Everything printed comes from the page -- nothing is invented.


--- wordnet.rs ---

# WordNet in-process source

Parses WNdb `index.*` and `data.*` files into memory at startup. Serves two purposes: definition fallback (synset glosses become `wordnet` candidates when Free Dictionary has nothing) and semantic clustering (hypernym chains give the plan builder a deterministic cluster key). When `wordnet_dir` is unset, the whole thing is absent: no fallback candidates and the semantic grouping stage is skipped.


============================================================
core/crates/reconcile/src/stages/
============================================================


--- aux_liveness.rs ---

Auxiliary word liveness stage. An auxiliary exists only because something points at it (dependency edge or distractor binding). When the last reference goes, it is retired; when a reference comes back, it is reactivated with assets intact. Both directions are reversible and destroy nothing. The `aux_liveness` view is a pure derivation; this stage diffs it against stored status.


--- distractors.rs ---

# Distractor binding stage

Binds the three nearest confusable words to each word, once. Product rule: the three distractors a word gets are the ones it keeps forever. Pool is target + active auxiliary. Selection prefers same-POS, then nearest Damerau-Levenshtein distance, ties broken by `(pos_mismatch, distance, frequency_rank, word_id)`. Morphological relatives (shared stem) are excluded. Also provides `plan_stem_rebinds` for detecting and fixing stem violations introduced by later word additions.


--- media_gc.rs ---

Media garbage collection marking stage. Reference set = candidate references + TTS references + release manifest references. Files outside the set get `gc_eligible_at = now + 14 days`; files that come back inside have the stamp cleared. Nothing is deleted -- the grace period must elapse and release manifests pin files forever.


--- mod.rs ---

# Stages module root

The inline maintenance sweep, run once per reconcile pass in fixed order: scoring -> selection -> OOS sync -> aux liveness -> distractors -> plan rebuild -> readiness -> media GC. Each stage reads what the previous one wrote. Every stage is idempotent and computes from a fresh read snapshot. Exports `SweepStats` for logging, plus public entry points for each stage.


--- oos.rs ---

OOV queue sync stage. Reconciles the human-facing `oos_queue` table with the `oos_occurrences` view (the truth). A set difference in both directions: tokens newly appearing in selected definitions open OOV entries; tokens no longer present auto-close them.


--- plan.rs ---

Plan rebuild stage. Computes `input_hash = blake3(algo_ver || params || ordered active ids || ordered edge set || grouping features)` from live state and compares with the current artifact. Equal means the plan is current and nothing happens. When stale, calls `graph::build_plan` and writes the new artifact. Grouping features are part of the hash so installing WordNet produces a new plan.


--- readiness.rs ---

Readiness recomputation stage. Gathers facts in one read, folds them through `readiness::evaluate_all`, and writes back only the rows whose verdict actually moved. Runs inline every pass -- readiness is pure DB math, not a job.


--- select.rs ---

# Scoring and automatic selection stage

1. Candidates whose `scorer_ver` is behind get rescored.
2. Empty slots take the highest-scoring available candidate.
3. Auto, unpinned slots switch only when a challenger clears the hysteresis margin.
4. Pinned slots are never touched.
5. The first sense a word gets is marked primary by strongest frequency evidence.
6. A reconciler-picked primary moves when evidence does; an editor-placed primary is never moved.

Example slots use a shared candidate pool across all three slots; selection assigns candidates to slots without duplication.
