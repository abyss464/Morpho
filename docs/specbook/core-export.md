# core/export

5 specs.


============================================================
core/crates/export/src/
============================================================


--- cut.rs ---

# Dependency Closure Pruning

From all words passing the quality gate, compute the "maximum releasable subset" — ensuring that every word in the release package has its referenced words also included in the package, with no broken links. Also generate a holdback report, sorted by impact, telling editors which word to fix first for the greatest benefit.

## compute(nodes, edges) → CutResult

Takes a set of word nodes and the dependency edges between them (definition dependencies + distractor bindings), and outputs the pruning result.

- `nodes`: each word's id, whether it passes the gate, the list of reasons for not passing, learning order.
- `edges`: `(from, to)` indicates that `from` must also be in the release package to be valid.

The algorithm uses fixed-point convergence: first put all gate-passing words into a set, then repeatedly remove any word that depends on a word outside the set, until it stabilizes. The result is independent of edge order and node order, always the unique maximum closed set.

The returned `CutResult` contains three parts:
- `exportable`: the final exportable set of word ids, in ascending order.
- `excluded`: all excluded words, each with a reason explanation, sorted by impact descending (ties broken by id ascending).
- `shippable_count`: the original gate-passing word count (before pruning).

## CutNode

The word node passed to `compute`. Fields: `word_id`, `shippable` (whether it passes the gate), `blockers` (reasons for not passing), `learning_order` (position in the learning plan).

## Holdback

A reason record for an excluded word. Fields: `word_id`, `root_cause` (cause code), `root_cause_detail` (human-readable explanation), `blocking_word_id` (which word blocked it, optional), `impact_count` (how many would-be releasable words were dragged down by this word).

## Constants

- `DEPENDENCY_HOLDBACK`: removal caused by closure pruning, with cause code `"dependency_holdback"`.
- `UNKNOWN_CAUSE`: fallback value when a word fails the gate but carries no cause code.

## Constraints

- Edges pointing to words that do not exist in the word database will directly prevent the source word from being released; they are not ignored.
- Self-loop edges are silently ignored and do not affect any results.
- Mutually dependent words (strongly connected components) sink or swim as a whole — if one fails, all fail.


--- error.rs ---

# exporterrortype

Defines all possible failure causes during the export process, and a convenient Result alias.

## ExportError

enum, covering the following cases:

- `Store` / `Sqlite` / `Io` / `Json`: Pass-through of the underlying dependency's error.
- `NoPlan`: No learning plan yet, cannot determine export order.
- `OutputExists`: Output directory already exists, refusing to override.
- `MissingMedia`: A media file is recorded in the database but cannot be found on disk.
- `UnreadableMedia`: The media file exists but reading failed.
- `MediaHashMismatch`: The media file's actual hash does not match the registry record.
- `MissingAsset`: A word that has passed a gate but lacks required assets (image or audio).
- `GatesFailed`: A hard gate was not passed, carrying a list of all failed items.

## ExportResult\<T\>

An alias for `Result<T, ExportError>`, uniformly used across the entire export crate.


--- lib.rs ---

# Exporter Entry Point

Top-level coordination module for the release export. Three-step pipeline: gatecheck → dependency closure pruning → package writing. Also defines two public entry points, preview and export, as well as hard validation gates.

## Submodules and Re-exports

- `cut`: dependency closure pruning algorithm (CutNode, CutResult, Holdback, DEPENDENCY_HOLDBACK)
- `error`: error types (ExportError, ExportResult)
- `model`: data loading (ExportPayload, ExportWord, GlossAnchor, REJECTED_SELECTION, STALE_EXTRACTION)
- `writer`: package writing (Manifest, ManifestEntry, WrittenRelease)

## preview(store, settings) → HoldbackReport

Corresponds to `GET /releases/preview`. Reads all word data from the working database, runs gates and pruning, and returns a holdback report. Does not write any files or modify any status. Report contents: plan id, passed/failed gate counts, exportable count, excluded count, reason and impact for each excluded word, and whether hard validation gates passed/failed.

## export(store, settings, out_dir, actor, notes) → (WrittenRelease, HoldbackReport)

Corresponds to `POST /releases/export`. First runs the preview pipeline; if hard validation gates fail, returns an error directly (GatesFailed). After passing, writes the release package to out_dir, then records the release in the working database (version number, hashes, media references, etc.) to prevent garbage collection from deleting referenced media files.

## validate(payload, cut) → Vec\<GateFailure\>

Hard validation gates. Checks the integrity of the pruning result — not word quality. If validation fails here, it means there is a bug in closure or readiness computation, and the release would crash the app. Checks:

- each exportable word must pass gates, have an image, and have word audio
- each exportable word has exactly one gloss item, and each gloss item has audio
- each exportable word has a slot-1 example, and each example has audio
- each exportable word has 3 distractors, all included in the release package
- the four images on each question card are distinct (content-addressed deduplication)
- every word token in definitions resolves to known words or word item anchors
- dependencies satisfy topological order (dependent words must be learned first or in the same group)

## GateFailure

A single hard gate failure record. Fields: gate (gate name), message (description), word_id (optional), lemma (optional).

## HoldbackReport / HoldbackEntry

Report structure shared by preview and export. HoldbackEntry is a single row/line in the report: word id, word form, role, reason, blocking word info, impact.

## ExportSettings

Configuration needed for a single export: TTS configuration, segmenter version, lemmatizer version, data directory path, exporter version string.

## Constraints

- Never continue writing the package when hard validation gates fail — these failures indicate system bugs, and the released APK would crash.
- preview is a read-only operation with no side effects.
- The output directory must not already exist; overriding or merging is not supported.


--- model.rs ---

# Export data model and loading

Defines all data structures required for the export package and provides a function to load them once from the working database. All data is pre-computed—no inference during loading, only reading and assembly.

## load(conn, tts, tokenizer_ver, lemmatizer_ver) → Option\<ExportPayload\>

Loads all data required for export in one pass from the working database connection. Returns None if no learning plan exists yet.

Loaded content: active vocabulary (with study order, images, audio, ready status), enabled senses, selected examples, plan groups, distractor bindings, dependency edges, word anchors, unparsed tokens, media registry.

Audio hashes are matched during loading according to TTS configuration: only audio with matching TTS input hash and ready status is adopted.

Tokenizer version and lemmatizer version determine whether each word's extraction is stale.

## ExportWord

Complete export view of a word. Key fields:

- `shippable()`: Determines if the word can ship. All four conditions must be met: core_ready is true, at least 3 distractors, extraction not stale, all selection slot candidates still available.
- `gate_blockers()`: Returns list of gate reasons why this word didn't pass. Appends reasons checked only at export time (stale extraction, unavailable selections, insufficient distractors), but filters out distractor readiness-related reasons (that's the closure pruner's job).

## ExportPayload

All input data for one export. Contains: plan_id, word list, sense list, example list, group list, distractor triples, dependency edges, word anchors, anchor references, unparsed tokens, media registry.

## ExportSense / ExportExample / ExportGroup / MediaEntry

Data structures corresponding to senses, examples, groups, and media files. All are flat value objects with no behavior.

## Constants

- `STALE_EXTRACTION`: Reason code used when the defined tokenization extraction lags behind the current tool version.
- `REJECTED_SELECTION`: Reason code used when a selection slot's candidate is no longer available.

## Constraints

- shippable() deliberately doesn't check distractor readiness—that's the closure pruner's job; checking here would make holdback report attribution meaningless.
- gate_blockers() actively filters out reason codes starting with `distractor_`, for the same reason.
- load performs no inference or recalculation, only reads results already computed by the reconciler.


--- writer.rs ---

# Release Bundle

Write a deterministic release bundle from the trimmed data. The same content exported on the same day produces byte-identical output files.

## Bundle Structure

```
<out>/release.db          SQLite database
<out>/manifest.json       Manifest file
<out>/img/{hash}.webp     Image
<out>/audio/{hash}.ogg    Audio
```

## write_bundle(out_dir, data_dir, payload, rows, date, exporter) → WrittenRelease

Write a complete release bundle. Procedure:

1. Copy all referenced media files to the output directory, verifying content hash for each one.
2. Compute content_hash from the data content (a pure content digest independent of the date).
3. Generate release.db (deterministic SQLite: page_size=4096, journal_mode=DELETE, insert in primary-key order, final VACUUM).
4. Write manifest.json (containing version, path/size/hash of all files).

Returns a WrittenRelease containing the output path, version number, content hash, database file hash, manifest, media hash list, and word count.

## rows_for(payload, exportable) → ReleaseRows

Filter and retain rows/lines from the full export data after trimming, ordered deterministically. Filtering logic:

- Words ascending by word_id
- Senses ordered by word_id, primary sense first, then sorted by part of speech
- Examples ordered by word_id and display order
- Groups: keep only those referenced by a word
- Distractors: keep only those where both sides are in the export set
- Word item anchors: keep only those actually referenced by an exported word (unreferenced ones are dropped)

## media_hashes(rows) → Vec\<String\>

Collect all media file hashes referenced by the given rows/lines set, deduplicated and sorted.

## content_hash(rows, media) → String

Compute a pure content digest from the exported rows/lines and media list. Covers words, senses, examples, groups, distractors, word item anchors, and media files. Independent of date; identical content always yields the same hash.

## content_version(date, content_hash) → String

Format: `YYYY.MM.DD+<first 8 chars of hash>`. Same content on the same day = same version number.

## write_release_db(path, rows, content_version, payload, date)

Create release.db from scratch. Create tables using the contract DDL, insert all rows/lines in primary-key order, write the meta table (version, date, plan id, schema version), then VACUUM to compact. Slot-1 examples carry the corresponding word's image path; other examples do not.

## media_path(kind, file_hash) → String

Return the relative path inside the bundle by type: image → `img/{hash}.webp`, otherwise → `audio/{hash}.ogg`.

## Manifest / ManifestEntry / WrittenRelease / ReleaseRows

- Manifest: structure of manifest.json, containing version, schema version, exporter, plan id, word count, media count, total bytes, and file listing.
- ManifestEntry: one file in the manifest, containing relative path, byte count, blake3 hash.
- WrittenRelease: summary of a single export's output.
- ReleaseRows: the trimmed and sorted rows/lines set, shared by write and hash operations.

## Constraints

- The output directory must not already exist; refuse to write if it does.
- Each media file must have its hash verified during copy; never trust the registry.
- No timestamps in the database (date only); otherwise two exports of the same content would differ.
- release.db's DDL comes from the contract file embedded at compile time; do not reinvent it.
