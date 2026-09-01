# core/morphod

7 specs.


============================================================
core/crates/morphod/src/
============================================================


--- config.rs ---

# Daemon configuration

Loads, merges, and distributes morphod's global configuration. All subsystems (storage, reconciliation, export, adapters, etc.) get their parameters from here.

## Priority

Command-line args > environment variables > config file > defaults. Secret-like parameters (e.g. API keys) only via environment variables, never in config files.

## Config

Root config structure containing these sections:

- `data_dir` — storage directory for working.db and media files
- `releases_dir` — output directory for export products
- `bind` — admin API listen address, default 127.0.0.1:8787
- `admin_ui_dist` — frontend static files directory
- `store` — read pool size, write queue depth, change broadcast capacity
- `reconcile` — full reconciliation interval (seconds), change merge window (milliseconds)
- `sources` — external data sources (WordNet, corpora, Unsplash, etc.)
- `adapters` — subprocess adapter root directory and startup parameters
- `tts` — text-to-speech config (voice, bitrate, etc.)
- `plan` — grouping parameters (min/max words per group)
- `images` — image chain behavior (scene mode, SDXL parameters)

## Config::load(explicit_path) → Config

Loads configuration. Search order: explicit path → `MORPHOD_CONFIG` environment variable → `morphod.toml` in current directory (if exists). No file found uses all defaults. After loading, automatically applies environment variable overrides, parses adapter root directory, and converts relative paths to absolute.

## Config::working_db() → path

Returns full path to working.db (`data_dir/working.db`).

## Config::engine_context() → EngineContext

Parses all external data sources and builds the context required by the reconciliation engine. Contains data source collection, media storage, TTS config, grouping parameters, image config, and text processing pipeline.

## Config::text_pipeline() → TextPipeline

Returns the combination of tokenizer and lemmatizer. Engine and exporter must share the same pipeline; version mismatch causes all words to be judged stale.

## Config::export_settings() → ExportSettings

Returns all settings needed by the exporter. Tokenizer and lemmatizer versions automatically stay in sync with the engine.

## Config::store_config() → StoreConfig

Translates the store section into a config structure the storage layer accepts.

## Config::reconciler_config() → ReconcilerConfig

Translates the reconcile section into a config structure the reconciler accepts.

## Config::repo_root() → path

Returns repository root (adapters/ parent directory). Falls back to current working directory if not found.

## Constraints

- Never write API keys or secrets in the config file; use environment variable overrides.
- The text pipeline for engine_context and export_settings must come from the same text_pipeline() call chain; version misalignment causes the exporter to falsely judge all words stale.
- Adapter root directory is parsed and fixed at load time; doesn't follow working directory changes afterward.
- Config file rejects unknown fields—an extra letter causes an error, not silent ignore.


--- export.rs ---

# Release Package export (CLI)

Implementation of the `morphod export` subcommand. Build a release package from the current state of the working database, or only preview which words will be excluded.

## run(config, store, out_dir, actor, notes, preview_only) → no return value

Run one export and print a human-readable summary.

- When `preview_only` is true, only compute the exclusion report, don't write any files.
- When `out_dir` is not specified, automatically create a directory under `releases_dir` with a timestamp.
- On export success, print: version number, package path, database hash, content hash, word count, media count, total bytes, and exclusion report.
- When a validation gate check fails, print the list of failure reasons and exit with an error, producing no output files.

## render_holdback(report) → string

Render the exclusion report as text. It contains three parts:

1. Overview line — publishable count, export count, excluded count
2. Root cause histogram — counts grouped by exclusion reason, sorted with the most frequent first
3. Most severe blocking word list — sorted by downstream impact, showing at most 20 items

When a validation gate fails, additionally display the failed gate's name and description. When there are no excluded words, output only the overview line.

## Constraints

- When a validation gate fails, never produce an output file; must exit with an error.
- The sorting of `render_holdback` is by downstream impact in descending order; do not change it to alphabetical order.


--- import.rs ---

# Word List Import

Read word list from file and write to working database.

## Supported Formats

A single file can mix both formats, detected per row/line:

- JSONL — each line is a JSON object, must contain `word` (or alias `lemma`), optional `phonetic`, `frequency_rank`
- Plain text — each line is a word

Empty rows/lines and comment rows/lines starting with `#` are skipped.

## parse_wordlist(path) → worditemslist

Read and parse word list file from disk. On JSON row/line format error, the error message will indicate the exact row/line number.

## parse_wordlist_str(raw, origin) → worditemslist

Parse word list from string; `origin` is used to identify the source in error messages. It shares the same parsing logic as parse_wordlist.

## import_wordlist(store, path, role) → Import Statistics

After parsing the word list, all word items are written to the working database in a single transaction. Returns counts of added, updated, unchanged, and skipped items. Re-importing the same word list is an idempotent operation.

`role` determines which role (target or base) the imported words belong to.

## Constraints

- An error is raised when the `word` field in a JSONL row/line is an empty string or pure whitespace; it will not be silently skipped.
- An error is raised when the file contains no valid word items; no empty transaction will be created.


--- main.rs ---

# morphod entry point

The only executable file of the Morpho content engine. A single process hosts the reconciliation loop, admin API, and exporters, sharing the same working database.

## Subcommands

When no subcommand is given, it defaults to running `serve`.

### serve

Starts the reconciliation loop and admin API. Optional parameters:
- `--bind` override the listen address
- `--admin-ui` override the frontend static file directory

### import

Imports a word list into the working database. Required parameters:
- `--wordlist` path to the word list file
- `--role` the role of the word, only accepts `target` or `base`

After the import completes, it prints the number of added, updated, unchanged, and skipped entries.

### status

Prints statistics about the working database, such as word count, asset override rate, and blocking distribution, then exits.

### export

Builds a release package from the working database. Optional parameters:
- `--out` output directory (defaults to auto-generated by timestamp)
- `--preview` only preview the exclusion report, do not write files
- `--actor` record the exporter (default `cli`)
- `--notes` notes stored with the release

### publish

Automated release pipeline: export → sync to Android project → run tests → build APK. Optional parameters:
- `--notes` notes for the export
- `--no-build` only export and sync, do not run Gradle
- `--actor` record the exporter (default `cli`)

## Global parameters

- `--config` specify the configuration file path
- `--data-dir` override the data directory
- `--log` log filtering level, default `info`

## Constraints

- `--role` only accepts `target` and `base`; passing any other value is rejected directly by clap.
- Running without a subcommand is equivalent to `serve`, not a no-op.


--- publish.rs ---

# Automated Release Pipeline

The `morphod publish` subcommand's implementation. It combines the six steps of manual release into a single command:

1. Export release package
2. Copy release.db to Android's assets directory
3. Sync media files (img/ and audio/), clean up stale files
4. Update version number in test assertions
5. Run Gradle build (tests + APK packaging)
6. Output summary report

## publish(config, store, notes, no_build, actor) → PublishResult

Executes the complete release pipeline.

- Aborts if the export validation gate fails, without proceeding to subsequent steps.
- When `no_build` is true, skips the Gradle build and only does export and sync.
- The returned PublishResult contains the version number, word count, media count, APK path, and size.

## PublishResult

Release result structure, containing:
- `content_version` — content version number
- `word_count` — number of word items
- `media_count` — number of media files
- `apk_path` — APK path (empty if build is skipped)
- `apk_size_bytes` — APK size (empty if build is skipped)
- `export_dir` — export package directory

## Media sync logic

Compares the export manifest against files under `app/content_media/src/main/assets/content_media/` in the Android project:
- Files in the manifest but not in the target — copy over
- Files already in the target with matching size — skip
- Files in the target but not in the manifest — move to trash using `gio trash`; if gio is unavailable, print the path and let the operator delete manually

## Test version number update

In `ReleaseDatabaseTest.kt`, look for a string literal matching the pattern `"20YY.MM.DD+8-digit hex"` and replace it with the new version number. Only the first match is replaced. If the file already has the latest version, it is not written.

## Gradle build

From the `app/` directory, run `./gradlew :domain:test :app:testFatApkDebugUnitTest :app:assembleFatApkDebug`. Exit with an error if the build fails. On success, find the `.apk` file in the build output directory and return its path and size.

## Constraints

- If the export validation gate fails, the entire pipeline must be aborted — do not proceed with sync and build using incomplete data.
- Media sync uses `gio trash` rather than direct deletion, preserving recoverability.
- Version matching is hand-written byte-level pattern matching, with a fixed format of `YYYY.MM.DD+HHHHHHHH` (19 characters); changing the format will cause matching to fail.


--- serve.rs ---

# Daemon Main Loop

Implementation of the `morphod serve` subcommand. Runs the rows/lines reconciliation engine and the admin API in a single process.

## serve(config, store) → no return value

Starts the daemon and blocks until a close signal is received. Does three things:

1. Start the reconciliation engine — runs rows/lines in a background task, continuously listens for data changes, and performs rows/lines reconciliation.
2. Start the admin API — binds to the configured address, mounts routes and static frontend files, and provides HTTP service externally.
3. Wait for the close signal — after receiving Ctrl+C or SIGTERM, gracefully closes the HTTP server first, then notifies the reconciliation engine to stop.

The API status contains: storage handle, reconciliation registry, data directory, export settings, publish directory, and repository database root directory.

## Constraints

- The close order must not be reversed: stop HTTP first, then stop the reconciliation engine, so that in-flight requests can finish processing.
- If the reconciliation engine exits abnormally, it only logs a warning and will not bring down the HTTP server.


--- status.rs ---

# Work Database Status Report

Implementation of the `morphod status` subcommand. Performs a one-shot query of the work database and prints a comprehensive statistical summary.

## StatusReport

Report structure, containing:
- `words` — word count by role (target, base, auxiliary, retired) and ready/blocked status
- `assets` — override rate by asset type (definition, example, image, TTS) (ready/missing/already abandoned) and candidate count
- `oos_open` — number of OOV (out-of-vocabulary) items pending in the queue
- `dead_letters` — number of dead letters (tasks that cannot be processed)
- `plan` — current plan (ID, build time, word count, group count); null when no plan exists
- `events` — total event count
- `releases` — number of exported releases
- `blockers` — histogram of blocking reasons, sorted by occurrence count in descending order

## collect(store, tts) → StatusReport

Reads all statistical data from the work database and assembles it into a report. The TTS override rate is calculated per configured voice using the unified ruling-#13 rule, ensuring consistent counts across the CLI, dashboard, and detail pages. The entire query completes in a single read-only transaction.

## StatusReport::render(db_path, sources, adapters) → string

Renders the report as text, additionally appending:
- External data source status (whether WordNet, corpus databases, etc. are configured)
- Adapter probe results — one row/line per adapter, indicating whether it is usable; when unusable, list the task types that would result in dead letters

## Constraints

- The deduplication and bucketing logic for the TTS override rate must remain consistent with ruling-#13; do not invent counting methods on your own.
- Adapter probing is part of the report (ruling-#17) and must not be omitted — unusable adapters must clearly indicate the affected tasks.
