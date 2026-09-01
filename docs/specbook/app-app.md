# app/app

62 specs.


============================================================
app/app/src/main/kotlin/dev/morpho/
============================================================


--- MainActivity.kt ---

# Main Activity

The app should use a single Activity. It is responsible for kicking off initialization, setting the theme, injecting CompositionLocal, and then handing control over to Compose navigation.

## Lifecycle behavior

- On creation, retrieve the container from MorphoApplication, call the container's async initialization, and show a loading state on screen until initialization completes.
- The theme switches in real time based on user settings (dark/light/follow system). The entire screen is redrawn at the moment of switching; pages don't need to be aware of it.
- The reduced-motion toggle is similarly read in real time from the settings stream.
- Through CompositionLocal, provide the content image renderer and term index to the entire Compose tree (before initialization completes, the index is empty, definitions first appear as plain text, and anchors are automatically acquired once the index is in place).
- When entering the background, perform a WAL checkpoint to ensure the auto-backup snapshot is self-consistent; also stop audio playback.
- When the Activity is destroyed and is truly finishing, close the container and release resources.


--- MorphoApplication.kt ---

# Should use entry point

Process-level Application subclass that holds a globally unique dependency container.

## container

An AppContainer instance shared by the entire process, created when the process starts. Activity, ViewModel, and other places that need dependencies all retrieve them from here. It is read-only and cannot be replaced externally.


============================================================
app/app/src/main/kotlin/dev/morpho/data/backup/
============================================================


--- ProgressBackup.kt ---

# Manual Progress Backup

Export and import progress via the Storage Access Framework. No storage permission is required — it relies entirely on the user's interaction with the system file picker.

Export: first checkpoint the WAL to ensure user.db is self-contained, then copy byte-for-byte to the user-selected location.

Import: stage the user-selected file next to the data database directory, validate it via ProgressBackupValidator, and after user confirmation, atomically replace the live database with rename, then restart the process.

## suggestedFileName(today) -> file name

Generates the suggested export file name in the format `morpho-progress-20260826.db`, sorted by date for easy lookup.

## export(target) -> bytes written or failure reason

Exports user.db to the given URI. Performs a WAL checkpoint first, then copies the file.

## stage(source) -> StagedBackup

Copies the user-selected file to a staging area and validates it. Nothing is replaced — the caller shows the validation result to the user for confirmation. If validation fails, the staged file is deleted automatically.

## discard(staged)

Discards the staged file when the user cancels the import.

## applyAndRestart(staged) -> boolean

Replaces the current user.db with the staged snapshot and restarts the process. The old database is renamed and kept first, with automatic rollback if replacement fails. On success, the entire process restarts and this method does not return. Returns false if the staged file is invalid, in which case nothing happens.

## restartProcess()

Restarts the current process. After replacing the database, all in-memory cached state is invalid, so a restart is the most reliable recovery method.

## StagedBackup (data class)

A candidate snapshot awaiting user confirmation: staged file path, validation result, and content summary.

## BackupSummary (data class)

An overview of the candidate snapshot's contents: number of tracked words, scheduled cards, and recorded days. Used in the confirmation dialog to inform the user of what will be restored.


--- ProgressBackupValidator.kt ---

# Backup file validator

Determines whether the file the user selected is a progress snapshot that the current build can restore. Importing overrides all of the user's progress, so validation runs before replacement, preferring to reject rather than take risks. The three checks are arranged by cost from low to high: whether it is a SQLite file, whether the four required tables are present, and whether the schema version number is compatible.

All methods are pure functions that don't rely on the Android runtime, so they can be directly validated with JVM unit tests.

## looksLikeSqlite(source) -> boolean
Reads only the first 16 bytes of the SQLite magic number header. Can instantly rule out files like images that the user accidentally selected. Accepts two overloads: an input stream or a file path.

## validate(file, tables, schemaVerRaw, supportedSchemaVer) -> BackupVerdict
Performs full validation. Pass in the candidate file, the already-read set of table names, the raw schema version string, and the version number supported by the current build. Returns the specific validation verdict.

## BackupVerdict (sealed interface)

Validation verdict; only Ok can proceed with import:
- Ok — validation passed, with the schema version number
- NotSqlite — not a SQLite file
- MissingTables — missing required tables, with the list of missing table names
- MissingSchemaVersion — the meta table has no schema_ver
- UnreadableSchemaVersion — the schema_ver value cannot be parsed as an integer
- NewerSchema — the file is from a build with a newer version; the current version cannot understand its new columns

### isOk (property)
A quick check for whether the verdict is Ok.


============================================================
app/app/src/main/kotlin/dev/morpho/data/content/
============================================================


--- ContentStore.kt ---

# Content-Addressed Media Storage

Resolves the media filenames recorded in release.db (such as `img/{hash}.webp`, `audio/{hash}.ogg`) to actual bytes. Callers don't need to know whether files come from Play Asset Delivery or are packaged directly inside the APK—both packaging methods go through the same interface. Thread-safe, fully offline, never touches the network.

## ContentStore (interface)

### open(name) -> input stream or null
Pass in a filename and get a byte stream. Returns null if the file doesn't exist in storage.

### openFd(name) -> file descriptor or null
Zero-copy method for obtaining a file descriptor, allowing Media3 to play directly without needing to extract the file from the APK first. Returns null if the file doesn't exist.

### uriFor(name) -> URI or null
Returns a URI that Media3 / Coil can use directly. Returns null if the file doesn't exist.

### exists(name) -> boolean
Checks whether the given filename exists in storage.

### handle(name) -> URI (static method)
Wraps a filename into an opaque handle in the `morpho://content/...` format, for consumption by Coil fetchers and Media3 data sources.

### nameFrom(uri) -> filename or null (static method)
The inverse of handle—extracts the filename from a `morpho://` URI. Returns null for URIs with a non-morpho scheme.

## AssetContentStore

The only production implementation. Reads media files from the `assets/content_media/` directory of the APK. Takes a Context at construction time, with an optional root directory name.


--- EtymologySegments.kt ---

# Word Source Segment Codec

Processes the JSON array in the `words.etymology_segments` column of `release.db` (e.g., `["bene","vol","ent"]`). This is the sole data source for the word source segmentation chip; when the column is null or has an invalid format, the interface degrades to plain-text word source display.

## parse(raw) -> string list
Decodes the column value into a segment list. Returns an empty list when the input is null, blank, malformed, or a non-array — no exceptions are thrown, it simply degrades silently.

## encode(segments) -> JSON string or null
Encodes the segment list back into a JSON array string. Blank segments are automatically filtered; returns null if the list is empty after cleanup. Note that this method does not write to `release.db` — it exists purely to validate round-trip consistency of encoding/decoding in tests.


============================================================
app/app/src/main/kotlin/dev/morpho/data/db/
============================================================


--- Databases.kt ---

# Database Provider

The app uses two SQLite databases. release.db is read-only and distributed with the APK; user.db is read-write, created from the schema generated by SQLDelight, and included in Android Auto Backup. Both databases use SQLDelight instead of Room, because release.db is produced by an external tool, and Room's schema management and migration mechanisms are pure obstacles here.

## contentDatabase (property)

The SQLDelight database instance for release.db. It is lazily initialized on first access — it automatically clears outdated old version files and installs the bundled release.db from assets.

## userDatabase (property)

The SQLDelight database instance for user.db. It is lazily initialized on first access — it detects and deletes old schema files from the pre-release stage.

## userDatabaseFile() -> filepath

Returns the absolute path to user.db, for use in export/import flows.

## contentDatabaseFile() -> filepath

Returns the absolute path to release.db.

## checkpointUserDatabase()

Flushes the WAL, making a single-file copy of user.db self-contained and complete. Must be called before backup or manual export. On failure, it silently logs and does not throw an exception.

## close()

Closes the drivers of both databases. After calling it, all higher layers holding database references become invalid. This is usually used only in the import flow before replacing files.


============================================================
app/app/src/main/kotlin/dev/morpho/data/haptics/
============================================================


--- HapticsManager.kt ---

# Haptic Feedback

According to the haptic mapping table in app-design.md, provide vibration feedback. Each haptic mode is always accompanied by a visual signal — vibration is never the only information channel. The global switch in settings controls the enable/disable of the entire layer.

## HapticPattern (enum)

Five modes:
- TAP — normal click, lightest confirmation
- CORRECT — correct, light touch
- WRONG — wrong, double vibration
- PROMOTE — mode promotion, crescendo double tap
- GROUP_COMPLETE — group completed, triple pulse success mode

## HapticsManager

Pass in Context when constructing.

### enabled (property, readable/writable)

Global switch. When set to false, all perform calls are silently skipped.

### hasVibrator (property, read-only)

Whether the current device has a vibrator.

### perform(pattern)

Trigger vibration by the specified mode. If enabled is false or the device has no vibrator, silently skip.


============================================================
app/app/src/main/kotlin/dev/morpho/data/media/
============================================================


--- AudioPlayer.kt ---

# Content Audio Player

Based on Media3's content audio playback. Maintains two players: the main player is the one that produces sound, and the preload slot prepares the next question's audio while the current question is being displayed, ensuring zero delay when switching questions. Audio files are read via ContentStore, independent of the specific packaging method.

## nowPlaying (StateFlow, read-only)
The filename of the currently playing file, or null when nothing is playing. The UI uses it to highlight the corresponding audio button.

## play(name)
Immediately plays the specified file, replacing the currently playing content. Silently skips if name is null, blank, or does not exist in ContentStore.

## preload(name)
Preloads the specified file on the standby player without producing sound. The next time the same file is played, it can start instantly. Repeated preloading of the same file is ignored.

## stop()
Stops the current playback.

## isPlaying() -> boolean
Whether the main player is currently producing sound. SoundManager uses this to determine whether it needs to lower the sound effect volume.

## release()
Releases the resources of both players. Cannot be used after being called.


--- MorphoImageLoader.kt ---

# image loading

Coil 3's project configuration. The network layer is intentionally not installed — no code path can make network requests. All images are read from local sources via/through ContentStore. Disk caching is also disabled, since the source files are already local, making secondary caching a pure waste.

## MorphoImageLoader.create(context, contentStore) -> ImageLoader
Creates a configured Coil ImageLoader instance. Enables memory caching (up to 20% of available memory), enables crossfade, and uses a ContentStore fetcher to resolve URIs in the `morpho://content/...` format.

## CoilContentImageRenderer

The production image renderer, implementing the ContentImageRenderer interface. In Compose Preview mode, it automatically falls back to a gradient placeholder renderer, so no actual resources are needed during previews.

### Image(file, contentDescription, modifier)
Compose component. Pass in the file name and accessibility description to render the corresponding content image. The image is cropped to fill its bounds, and the semantic description is bound to the outer container.


============================================================
app/app/src/main/kotlin/dev/morpho/data/repository/
============================================================


--- ContentRepository.kt ---

# Content Database

A read-only access layer for release.db, mapping database rows to domain models. The content database does not change during runtime; all methods are ordinary suspend reads, with no data flow listening and no cache invalidation needed. Learning plans and shipped word collections are cached because every build session needs them.

## planWords() -> plan word list
Returns the learning plan slice for all words in release.db, containing word ID, group ID, and learning order. The result is cached.

## shippedWordIds() -> set of word IDs
All word IDs shipped in the current release.

## wordCount() -> integer
Total number of word items in release.db.

## glossIndex() -> definition index
Loads the entire gloss_anchors table as a lookup index. The result is cached for the process lifetime—the content database is immutable, and item-by-item queries would be too expensive.

## isEmpty() -> boolean
Whether release.db contains no word items.

## contentVersion() -> version string or null
Reads the content version number from the meta table.

## metaValue(key) -> value or null
Reads an arbitrary item from the meta table by key.

## group(groupId) -> word group or null
Loads a word group by group ID.

## word(wordId) -> word or null
Loads a word item by word ID.

## words(wordIds) -> word list
Batch loads multiple word items. An empty input collection returns an empty list.

## bundle(wordId) -> word bundle or null
Loads a word along with its definitions, examples, and three bound distractor word IDs. The release build guarantees the distractor word closure is complete.

## bundles(wordIds) -> mapping from word ID to word bundle
Batch version—each table is queried only once, rather than once per word.

## questionBundles(wordId) -> QuestionContent or null
Loads all the data a question needs at once: the answer word bundle plus the word bundles of the three distractors. If the distractor word closure is incomplete, logs an error and returns null.

## assertIntegrity() -> violation description list
Full integrity scan for debug builds. Returns a human-readable violation list; an empty list means the release data is intact. Checks include: uniqueness of definition main marks, distractor word count, dangling distractor word references, missing first example, example highlight range out of bounds, conflicts between definition anchors and already-published words, and learning order uniqueness.

## QuestionContent (data class)
The complete content of a question: answer is the answer word bundle, and options contain the answer itself and the word bundles of the three distractors in order.


--- ProgressRepository.kt ---

# Learning Progress Repository Database

Read/write access layer for user.db. Progress rows/lines are never deleted — when a future release removes a word, those rows/lines are only disassociated from release.db; if the word is published again later, the FSRS status is restored exactly as before.

## Learning Progress

### observeProgress() -> progress list data flow
Observes all learning progress rows/lines as a reactive Flow. Automatically pushes new values when the database changes.

### allProgress() -> progress list
Reads all learning progress rows/lines in one go.

### progressFor(wordIds) -> map from word ID to progress
Batch queries learning progress for specified words. Returns an empty map for an empty input set.

### progressMap() -> map from word ID to progress
All progress rows/lines, keyed by word ID.

### learnedCount() -> integer
Number of words already learned.

### upsertProgress(rows)
Batch writes learning progress. Updates if a word ID exists; inserts if it doesn't. Completed in a transaction.

## FSRS card

### allCards() -> card list
All FSRS scheduling cards.

### card(wordId) -> card or null
Queries a single word's FSRS card.

### dueCards(now) -> card list
Cards due for review at the current time.

### dueCount(now) -> integer
Number of cards due at the current time.

### upsertCards(rows)
Batch writes FSRS cards. Completed in a transaction.

## Daily Statistics

### statsFor(date) -> stats or null
Queries learning statistics for the specified date.

### recentStats(limit) -> stats list
Recent daily statistics, at most 400 items by default, sorted by date descending.

### upsertStats(row)
Writes one day's statistics data.

## Metadata

### metaValue(key) -> value or null
Reads a key value from the meta table.

### setMeta(key, value)
Writes a key value to the meta table.

### ensureInitialised(contentVersion)
Ensures user.db metadata is initialized: writes the current schema version number, sets a default daily goal (if not yet set), and records the content version number. Completed in a transaction.

### clearAll()
Clears all tables. Only used in the import flow before restoring a snapshot. Completed in a transaction.


--- SettingsRepository.kt ---

# Settings Repository Database

Should use the settings read/write layer. All preferences are stored in the meta table of user.db rather than DataStore, so that Android Auto Backup and manual progress export automatically override the settings, without needing to maintain a second storage mechanism.

## MorphoSettings (Data Class)

A snapshot of all settings, containing: daily goal word count, sound toggle, haptics toggle, SFX volume, reduced motion override, activity chart style, theme mode. Each field has a reasonable default value.

## SettingsRepository

### settings (StateFlow, read-only)

A reactive stream of the current settings snapshot. The UI layer observes it to drive the UI state.

### load() -> Settings Snapshot

Loads all settings from user.db, updates the StateFlow, and returns the load result. When values are missing or malformed, default values are used.

### setDailyGoal(value)

Sets the daily goal word count. Automatically clamps to the range of 10–200. Simultaneously writes to the database and updates the StateFlow.

### setSoundEnabled(value)

Toggles the sound switch.

### setHapticsEnabled(value)

Toggles the haptics feedback switch.

### setSfxVolume(value)

Sets the SFX volume. Automatically clamps to the range of 0–1.

### setReducedMotion(value)

Sets the reduced motion override. Pass null to indicate following the system setting.

### setActivityChartStyle(value)

Sets the display style of the activity chart.

### setThemeMode(value)

Sets the theme mode.


============================================================
app/app/src/main/kotlin/dev/morpho/data/sound/
============================================================


--- SoundManager.kt ---

# UI Sound Effects Management

Based on SoundPool for UI sound effect playback, managed separately from content audio—the two channels each control volume independently. All sound effects are pre-decoded and kept in memory before first use, ensuring latency below 150ms. When Media3's content audio is playing, sound effects are automatically reduced to 0.6x volume to prevent celebration sounds from overpowering definition reading.

## SfxEvent (enum)

Seven sound effect events:
- TAP — Light tap, barely perceptible
- CORRECT — Correct, short rising xylophone notes
- WRONG — Wrong, low muffled thud
- PROMOTE — Mode promotion, dual rising notes
- GROUP_COMPLETE — Group complete, three-note trumpet
- REVIEW_DONE — Review queue cleared, soft bell sound
- STREAK — Streak advancement, shimmering sound

## SoundManager

Accepts a Context at construction, with an optional callback to determine whether content audio is currently playing.

### enabled (property, readable/writable)
Global master switch. When disabled, all play calls silently skip.

### volume (property, readable/writable)
Volume, between 0 and 1. Automatically clamped when set.

### preload(context)
Pre-decodes all sound effect files. Called once at startup. Missing sound effect files log a warning but do not block.

### play(event)
Plays the specified sound effect. Silently skips when enabled is false or volume is zero. Also silently skips if the sound effect has not finished loading.

### release()
Releases SoundPool resources. Cannot be used after calling.


============================================================
app/app/src/main/kotlin/dev/morpho/di/
============================================================


--- AppContainer.kt ---

# Dependency Container

Manual dependency injection container. All dependencies are explicitly wired in one file. No annotation processors are used. All properties are lazily loaded and created on first access.

## Exposed Dependencies

### Storage Layer
- **databaseProvider** — Database provider, manages opening/closing/checkpointing of the content database and user database.
- **contentRepository** — Content repository, reads teaching materials such as words, examples, and images from the release package.
- **progressRepository** — Progress repository, reads and writes the user's learning progress, daily statistics, and FSRS cards.
- **settingsRepository** — Settings repository, reads and writes user preferences (daily goal, theme, sound effects, etc.).
- **progressBackup** — Progress backup, exports/imports user data database.

### Media Layer
- **contentStore** — Asset store, reads images and audio by file name from assets.
- **audioPlayer** — Audio player, plays word and example pronunciations, exposes a Flow of the currently playing file.
- **soundManager** — Sound effect manager, plays UI sound effects such as correct/wrong/level-up.
- **hapticsManager** — Haptic feedback manager.
- **imageLoader** — Image loader (Coil), internally fetches images from contentStore.
- **contentImageRenderer** — Content image renderer, for use by the Compose layer.

### Domain Layer
- **reviewScheduler** — Review scheduler, decides which cards are due and how to score them based on the FSRS algorithm.
- **sessionResults** — Session result holder, written after learning/review sessions end, read by the summary page.

## initialize() -> StartupReport

One-time startup work off the main thread. Opens the release package, verifies content version, loads the term index, reads settings, and preloads sound effects. Debug builds also run a full integrity assertion scan. Returns a report containing the total word count, content version number, term index, and list of integrity violations.

## playSfx(event)

Plays a UI sound effect event.

## shutdown()

Releases the audio player and sound effect manager, closes the data database. Called when the process ends.

## StartupReport

Initialization result, contains: total word count, content version number, term index, and list of integrity violations.

## SessionResultHolder

In-memory intermediary between learning/review sessions and the summary page.

- **publish(result)** — Writes a result when a session ends.
- **consume()** — The summary page reads the most recent result.

## SessionResult

A statistical snapshot of a session: session kind (learning/review), number of newly learned words, number of reviewed items, number of first-attempt correct answers, total first-attempt answers, consecutive days, and whether the goal was met. Provides a computed property `accuracy` (first-attempt correct rate, empty when there are no answers).

## SessionKind

Session kind enum: LEARNING (learning), REVIEW (review).


============================================================
app/app/src/main/kotlin/dev/morpho/ui/
============================================================


--- MorphoApp.kt ---

# Navigation Graph

Defines all page routes to use and the Compose NavHost.

## Route Constants (MorphoRoutes)

- **HOME** — Home page
- **LEARN** — Learning session
- **REVIEW** — Review session
- **SETTINGS** — Settings page
- **SUMMARY** — Session summary page, with a session type parameter (LEARNING or REVIEW) in the path

## MorphoApp(container, startup, navController?)

Top-level Composable that receives the dependency container and startup report. Shows a loading animation when startup is empty; renders the navigation graph once ready.

Page transitions use shared-axis animation, with forward and backward directions automatically reversed.

### Page Flow
- From Home, you can go to: Learning, Review, Settings
- After Learning/Review ends, navigate to the summary page of the corresponding type, while clearing the back stack back to Home
- The summary page can go back to Home, or choose to continue Learning
- Settings page returns by popping


============================================================
app/app/src/main/kotlin/dev/morpho/ui/common/
============================================================


--- Mappers.kt ---

# Domain-to-Presentation Layer Mapping

## WordBundle.toWordDetail() -> WordDetail

Converts the word package in the domain model into the presentation model required for the detail overlay. It does three things: takes the data's byte offset and converts it to a Kotlin character range (used for example highlighting), parses the word source's segmented JSON into a tag list, and reassembles senses and examples into the presentation structure. The caller can directly pass the result to the detail component.


============================================================
app/app/src/main/kotlin/dev/morpho/ui/designsystem/component/
============================================================


--- AnswerFeedbackOverlay.kt ---

# Answer Feedback Overlay

A full-screen semi-transparent feedback layer, overlaid on top of the answering interface, using visual flashes and celebration animations to tell the user whether they answered correctly, answered incorrectly, were promoted, or completed a group.

## FeedbackSignal

The signal to be broadcast when the overlay is active, with five possible values:

- **None** — No feedback; the overlay is transparent and invisible.
- **Correct** — Correct; flashes a primary-color glow.
- **Wrong** — Wrong; flashes a red glow.
- **Promoted(mode)** — Promoted to the specified mode; flashes a tertiary-color glow. `mode` is the mode number after promotion.
- **GroupComplete(message)** — When the current learning group is fully complete, shows a celebration animation and prompt text.

## AnswerFeedbackOverlay(signal, onCelebrationFinished, modifier, celebrationAsset)

Displays the answer feedback overlay.

- **signal** — The current feedback signal; determines the flash color and whether to play the celebration animation.
- **onCelebrationFinished** — Callback invoked after the celebration animation finishes (either playing to the end naturally or skipped by the user clicking).
- **celebrationAsset** — Optional Lottie animation asset name; if not provided, the built-in butterfly-blue halo animation is used instead.
- The celebration animation lasts at most 1.5 seconds; clicking anywhere can skip it early.
- Wrong / Correct / Promoted only perform a brief flash and do not block interaction.


--- AudioChipButton.kt ---

# Audio Playback Button

A small circular button. Click to play audio for a piece of content (word pronunciation, definition reading, etc.). While playing, a pulsing ring animation is shown so users can tell which segment is currently playing.

## AudioChipButton(onClick, modifier, playing, enabled, small, contentDescription)

- **onClick** — click callback
- **playing** — whether it is currently playing; when true, the button changes color, the icon turns into a waveform, and a pulsing ring expands outward
- **enabled** — whether it can be clicked
- **small** — use the smaller size variant (suitable for embedding in definition rows/lines or example cards)
- **contentDescription** — accessibility label, defaults to "Play pronunciation"
- The pulse animation is automatically disabled when the system "Reduce Motion" setting is on, leaving only the icon switch


--- ContentImage.kt ---

# ContentImage

An abstraction layer between components and the image loading pipeline. Components only need to pass a content-addressed file name (e.g., `img/{hash}.webp`), and the runtime environment decides how to load and render it.

## ContentImage(file, contentDescription, modifier)

Displays a content image.

- **file** — image file name (content-addressed path)
- **contentDescription** — accessibility description; pass null to indicate a purely decorative image
- Actual rendering is provided by `LocalContentImageRenderer`; the App uses Coil to load real images from ContentStore, while Previews and tests automatically fall back to deterministic gradient blocks generated from the file name.

## ContentImageRenderer

Renderer interface, with the only method `Image(file, contentDescription, modifier)`. The App layer provides the real implementation and injects `LocalContentImageRenderer`.

## LocalContentImageRenderer

CompositionLocal whose default value is the gradient block renderer. At App startup, it is replaced with the Coil implementation.


--- DefinitionBlock.kt ---

# definition block

A definition component: part of speech tag + serif definition text + independent read-aloud button. On the detail page, this component is stacked item by item for the selected word's meanings.

## DefinitionBlock(pos, definition, onPlayAudio, modifier, playing, isPrimary)

- **pos** — part of speech abbreviation (e.g. "adj", "noun"), displayed as a small tag
- **definition** — English definition text, supports gloss anchors (clicking a word pops up the Chinese definition)
- **onPlayAudio** — callback for clicking the read-aloud button
- **playing** — whether the definition audio of this item is currently playing
- **isPrimary** — whether it is marked as the primary meaning; when true, the text "primary" appears next to the part of speech tag


--- DetailSheet.kt ---

# Word Detail Page

A modal panel that pops up from the bottom, displaying a word's complete information: title, image, all selected definitions, examples, and word origin. It is forced to appear when the answer is wrong, and also appears when a word graduates.

## WordDetail

All data required for the detail page, already parsed from the publishing database:

- **wordId** — word ID
- **word** — word spelling
- **phonetic** — pronunciation, nullable
- **imageFile** — image file name
- **wordAudioFile** — audio file name for word pronunciation
- **senses** — definition list (SenseDetail)
- **examples** — example list (ExampleDetail)
- **etymology** — word origin description text, nullable
- **etymologySegments** — list of word root/affix split segments (e.g. ["bene", "vol", "ent"])

## SenseDetail

A single sense definition: pos (part of speech), definition (definition text), isPrimary (whether it's the main sense), audioFile (audio file name for this sense).

## ExampleDetail

A single example: sentence (full sentence), highlight (character range of the target word in the sentence), audioFile (audio file name for the example).

## DetailSheet(detail, onDismiss, onPlay, modifier, playingFile, continueLabel, onContinue)

M3 modal bottom panel, displaying complete word details.

- **detail** — WordDetail data
- **onDismiss** — callback to close the panel
- **onPlay** — callback to play audio, parameter is the audio file name
- **playingFile** — audio file name currently playing, used to highlight the corresponding play button
- **continueLabel** — bottom button text (e.g. "Got it"), pass null to hide the button
- **onContinue** — callback for the bottom button

## DetailSheetContent(detail, onPlay, modifier, playingFile, continueLabel, onContinue)

The panel content body, extracted separately so it doesn't depend on the bottom sheet host in Preview. Parameter meanings are the same as DetailSheet.

## EtymologyChips(segments, modifier)

Renders word root/affix segments as tags/rows/lines joined with plus signs (e.g. `bene` + `vol` + `ent`).


--- GlossedText.kt ---

# Glossed Text

An English text component that automatically detects gloss anchor words and marks them with a light underline. When triggered by the user, it pops up a Chinese definition bubble. Product position: English carries the meaning, and the Chinese definition is a last resort, just like images.

## LocalGlossIndex

CompositionLocal that holds the current release's gloss_anchors index. Defaults to an empty index; renders as plain English in Preview and tests.

## GlossTrigger

The trigger method for anchor words:

- **Tap** — click to trigger; used for non-answer text (detail page definitions, examples)
- **LongPress** — long-press to trigger; used for answer option cards. Since clicking an option card is already used for "select answer," long-press avoids ambiguity

## GlossedText(text, style, color, modifier, trigger, enabled, onPlainTap, pressInteractionSource, maxLines, overflow, textAlign)

Displays English text with gloss anchor marks.

- **text** — the text content; accepts AnnotatedString or plain String (two overloads)
- **style** — text style
- **color** — text color
- **trigger** — anchor trigger method; defaults to Tap
- **enabled** — whether to respond to gestures
- **onPlainTap** — callback when a tap lands on a non-anchor area (e.g., playing audio, selecting an option)
- **pressInteractionSource** — the outer card's InteractionSource; when passed in, the text area's press animation syncs with the card
- **maxLines / overflow / textAlign** — standard text layout parameters
- When no anchor is hit and no onPlainTap is provided, it degrades to a plain Text component with zero extra overhead
- Anchor words are marked with a tertiary color and a thin underline; the popup bubble shows the word item's base form and Chinese definition


--- Indicators.kt ---

# Progress Indicators

Three progress feedback components: mode dots, group progress items, and the home progress ring.

## ModePips(mode, modifier, total)

Mode step indicator (1 → 2 → 3), three small dots.

- **mode** — the current mode (1-based)
- **total** — total number of modes, defaults to 3
- Reached dots are filled with the primary color, while the currently active dot is enlarged with a glow; a 400ms fill animation plays when advancing

## GroupSegmentState

The status of each word in the group progress items: PENDING (not started), IN_PROGRESS (in progress), PASSED (already passed), CARRIED (carried over from the previous session).

## GroupProgressBar(segments, modifier)

Segmented progress items, one segment per word, so users can see at a glance how much of the group remains.

- **segments** — list of each word's status
- Each segment is colored by status: passed ones use the primary color fill, carried-over ones use the streak color, in-progress ones are semi-transparent, and not-started ones use the base color

## ProgressRing(progress, centerLabel, centerCaption, modifier, accessibilityLabel)

Home page ring progress indicator with a gradient arc and central statistics.

- **progress** — the progress value from 0 to 1
- **centerLabel** — the large text in the center of the ring (e.g., "2,090")
- **centerCaption** — the caption below the large text (e.g., "of 5,500 learned")
- **accessibilityLabel** — accessibility label, defaults to combining centerLabel and centerCaption
- The arc expands with a 600ms ease-out animation


--- MorphoLoader.kt ---

# Morpho Loading Animation

Taken from the app icon's four marks (fog blue square, copper diamond, two parchment diamonds), made into a compact loading indicator. The four marks sequentially scale up and fade in, flowing from left to right like waves, then looping.

## MorphoLoader(modifier)

Displays a loading animation. No parameters need to be passed; colors and shapes are read from the theme.

- Under the system's "Reduce Animation" setting, the scale effect is disabled, keeping only the fade in/out
- The accessibility label is fixed to "Loading"


--- Motif.kt ---

# Icon Mark System

The four marks below the M in the icon (square → diamond → diamond → diamond) are elevated from decoration to formal components. Read from left to right as "unlearned → learning → mastered" — the entire app's learning model is condensed into four shapes.

## MarkKind

Mark shapes: SQUARE or DIAMOND.

## MorphoMark(kind, color, modifier, size)

An individual icon mark that can be placed inline next to a label.

- **kind** — shape
- **color** — color
- **size** — size, default 10dp

## MOTIF_MARK_COUNT

The number of marks used by all wide progress rows/lines, fixed at 12. One mark is approximately 1/12 of the target.

## MotifProgressRow(fraction, modifier, markCount, cell, contentDescription)

Progress rows/lines drawn using icon marks.

- **fraction** — progress value from 0 to 1
- **markCount** — total mark count, default 12
- **cell** — cell size for each mark, default 14dp
- **contentDescription** — accessibility label
- Completed steps are solid diamonds; in progress rows/lines, the current step is a copper diamond (visual focus); unstarted steps are mist-blue squares; spacing is computed automatically to fill the parent container

## MotifMilestoneRail(fraction, modifier, milestones, contentDescription)

A thin rail-style progress item, suitable for long-arc scenarios such as vocabulary size. Milestones sit on the rail as diamonds.

- **fraction** — progress value from 0 to 1
- **milestones** — milestone position list, default [0.25, 0.5, 0.75]
- **contentDescription** — accessibility label
- Reached milestones are solid copper diamonds; unreached ones are outlined diamonds

## SectionHeading(text, modifier, trailing)

Section title rows/lines: all-caps label + thin line extending to the right edge + optional trailing control.

- **text** — title text
- **trailing** — optional control at the end of the title row/line

## MotifOrnament(modifier)

Centered decoration: thin line — copper diamond — thin line. Used at the end of a group of sections.

## MotifSignature(modifier, size)

A static arrangement of the icon's four marks — square, diamond, diamond, diamond. Used as a brand auxiliary identifier.

- **size** — size of each mark, default 8dp


--- Previews.kt ---

# Preview Tools

All design system components share the same preview infrastructure.

## ThemePreviews

Annotation applied to @Preview functions that automatically generates two previews — light and dark (width 400dp).

## PreviewBox(darkTheme, content)

Component preview container. Automatically wraps with MorphoTheme and Surface, with standard padding on all sides.

- **darkTheme** — whether to use dark theme, defaults to following the system

## ScreenPreviews

Annotation that generates four full-screen previews: light, dark, short screen (592dp), landscape (780x380dp). Used to test how bottom-anchored layouts perform on different screen shapes.

## ScreenPreviewBox(darkTheme, content)

Full-screen preview container. No padding added — screen components have their own spacing built in.

- **darkTheme** — whether to use dark theme, defaults to following the system


--- QuizImageGrid.kt ---

# Image Options Grid

A 2x2 image grid, used for the answer interface of mode 1 (pure image) and mode 2 (image + definition caption).

## ImageOption

An image option's data:

- **wordId** — word ID
- **imageFile** — image file name
- **caption** — optional English definition caption (only used in mode 2)
- **accessibilityLabel** — accessibility label

## QuizImageGrid(options, onSelect, modifier, selectedIndex, correctIndex, revealed, enabled, space)

Mode 1 grid: 2x2 pure images, no text.

- **options** — the four image options
- **onSelect** — callback when an option is selected; parameter is the option index
- **selectedIndex** — the index the user has already selected
- **correctIndex** — the index of the correct answer
- **revealed** — whether the answer has already been revealed
- **enabled** — whether it is interactive
- **space** — available space passed down from QuizLayout; the grid adapts cell sizes accordingly; if not passed, each cell maintains a 4:3 natural aspect ratio
- When pressed, scales down to 0.97; when correct, a blue border + checkmark badge pops in and the other options fade to 0.4; when wrong, shakes horizontally by ±8dp

## QuizImageDefGrid(options, onSelect, modifier, selectedIndex, correctIndex, revealed, enabled, space)

Mode 2 grid: the same 2x2 images, with a line of serif definition caption below each image. The parameters and interaction behavior are consistent with QuizImageGrid. The gloss anchor in the caption area is triggered via long press (because clicking is already used for "selecting the answer"). The number of caption lines adapts to the longest item's definition in the current question, but will not compress the image below the minimum usable height.


--- QuizLayout.kt ---

# Quiz Layout Skeleton

All quiz pages share the same structure: the answer area is anchored to the bottom of the screen (thumb zone) and never moves or gets squeezed when other content appears.

## QuizAnswerSpace

The answer area is allocated its actual space: maxWidth (width) and maxHeight (maximum height). After being measured by QuizLayout, it is passed to the answers slot, letting the grid adjust cell sizes accordingly rather than guessing screen dimensions from rows/lines.

## QuizLayout(modifier, header, banner, prompt, answers)

Four slots, from top to bottom:

- **header** — fixed at the top (usually holds grouped progress items)
- **prompt** — flexible area, consumes all remaining space, scrolls automatically when content overflows
- **banner** — sits right above the answer area (e.g., retry hint card); when it appears, it compresses the prompt area instead of pushing the answer area
- **answers** — anchored at the bottom, height limited to 62% of the viewport; receives the QuizAnswerSpace parameter

Adaptation for three screen shapes:

- **Portrait** — standard four-layer stack, answer area pinned to bottom
- **Landscape** — left/right split, question on left for vertical reading, answer area on right
- **Unbounded height** (@Preview or wrap-content host) — falls back to single-column scrolling


--- QuizOptionState.kt ---

# Answer Option States

Three answer grids (image, image+definition, plain text) share the same option visual state enum.

## QuizOptionState

- **IDLE** — initial state, not selected
- **PRESSED** — user is currently pressing down (scaled to 0.97, 100ms)
- **CORRECT** — user selected correctly (pop-out border + checkmark badge, 250ms spring animation, other options fade out)
- **WRONG** — user selected incorrectly (horizontal shake ±8dp, 300ms spring animation, option dims)
- **DIMMED** — after the answer is revealed, unselected incorrect options (faded to 0.4 opacity)
- **REVEALED** — after the user selects incorrectly, the option containing the correct answer (displays border + checkmark, so the user can identify the correct answer)

## DIMMED_OPTION_ALPHA

The opacity of unselected options after the answer is revealed, fixed at 0.4.


--- QuizTextOptions.kt ---

# Text-Only Option List

Mode 3's answer interface: four vertically stacked definition cards, no images, no examples. Also serves the "definition-to-word" review type.

## TextOption

Data for a text option:

- **wordId** — word ID
- **text** — option text (definition or word)
- **pos** — optional part of speech abbreviation, displayed as a small tag
- **serif** — whether to use a serif font (definition options: true; word options: false)

## QuizTextOptions(options, onSelect, modifier, selectedIndex, correctIndex, revealed, enabled)

Vertically stacked text option card list.

- **options** — list of options
- **onSelect** — callback for selecting an option; takes the option index as a parameter
- **selectedIndex** — index of the option the user has already selected
- **correctIndex** — index of the correct answer
- **revealed** — whether the answer has already been revealed
- **enabled** — whether it is interactive
- Interaction animations match the image grid: scale down on press, border pop on the correct answer, shake on the wrong answer
- The gloss anchor in the card is triggered via long press (click = select answer)

## PosChip(pos, modifier)

Part of speech tag pill, shared by the answer card and the detail page. Displays the part of speech abbreviation text with a secondaryContainer-colored background.


--- RetryHelpCard.kt ---

# retry help card

When the user answers incorrectly, a teaching card pops up, using the easiest-to-accept moment for learners to convey complete information instantly. It is displayed directly above the answer area (the QuizLayout's banner slot), using the error container's color scheme. When the content exceeds the maximum height, scroll internally.

## RetryHelpCard(hint, word, phonetic, senses, onPlayWord, modifier, playing, maxHeight)

- **hint** — hint text (e.g., "Not this one. Read the meaning, then pick again.")
- **word** — the target word
- **phonetic** — pronunciation, nullable
- **senses** — the full list of selected definitions for this word (SenseDetail list), with part-of-speech tags; the primary sense has a "primary" marker
- **onPlayWord** — callback for playing the word's pronunciation
- **playing** — whether the pronunciation is currently playing
- **maxHeight** — the card's maximum height, default 300dp; scrolls internally when exceeded
- The card as a whole uses the errorContainer color scheme, and the definitions are rendered as plain text (without gloss anchors — the same definitions can be found in the detail page with links; after an incorrect answer, the detail page is forcibly shown)


--- SentenceCard.kt ---

# examplecard

Mode 1's question stimulus: a floating card displays the English example in serif font, with the target word highlighted as a pill using the primary color background. Click anywhere on the card to play the example audio.

## SentenceCard(sentence, highlight, onPlayAudio, modifier, playing, compact)

- **sentence** — the complete example text
- **highlight** — the target word's character range in the sentence (note: it's a character offset, not a UTF-8 byte offset; use `Example.highlightCharRange()` to convert before calling)
- **onPlayAudio** — callback for playing the example audio
- **playing** — whether currently playing
- **compact** — compact mode (uses a smaller font size in the example list on the detail page)
- The example text supports gloss anchors; clicking an anchor word pops up its Chinese definition; clicking non-anchor areas plays audio
- There is a small audio play button in the bottom right corner


--- SpellInput.kt ---

# Spelling Input Box

A letter-grid input component for listen-and-spell review questions. Each letter occupies one cell; keyboard input is captured by a hidden text box, while the visible cells are for display only.

## SpellState

Spelling status: TYPING (input in progress), CORRECT (spelled correctly), WRONG (spelled incorrectly).

## SpellInput(target, value, onValueChange, onSubmit, modifier, state, enabled, revealAnswer)

- **target** — Target word (determines the number of cells)
- **value** — The string the user has currently entered
- **onValueChange** — Input-change callback; automatically filters to keep only letters, hyphens, and apostrophes, with length no longer than the target word
- **onSubmit** — Callback fired when the user presses "Complete" on the keyboard
- **state** — Current spelling status
- **enabled** — Whether input is enabled; when true, automatically requests focus to bring up the keyboard
- **revealAnswer** — Whether to reveal the correct answer letter-by-letter (used after a wrong spelling)
- Each letter fades in and floats up into place; on a wrong spelling, the entire row shakes horizontally
- While typing: the current cursor cell is marked with a bold primary-color border
- Correct: all cell borders turn to the primary color
- Wrong: borders turn to the error color; when revealAnswer is true, the correct letters are shown cell by cell (40ms delay per cell), and the wrongly-spelled letters are displayed in the error color


--- Stats.kt ---

# Statistics Components

Statistics display components used on the home page and learning summary page.

## StreakBadge(days, modifier)

Badge showing consecutive learning days, with warm colors to convey a sense of reward.

- **days** — Number of consecutive days
- Displays a flame icon + the day count + "day"/"days"

## StatTile(value, label, modifier, icon, emphasis)

A single data tile: a large number + descriptive text, with an optional icon.

- **value** — Numeric text (e.g., "18", "94%")
- **label** — Descriptive text (e.g., "new words", "accuracy")
- **icon** — Optional icon at the top
- **emphasis** — Whether to use an emphasized style (uses primaryContainer background)

## SessionSummaryCard(headline, supporting, modifier, content)

Summary card shown when learning/review ends.

- **headline** — Title (e.g., "Group cleared")
- **supporting** — Subtitle/descriptive text
- **content** — Additional content slot inside the card

## IconPill(icon, contentDescription, modifier)

Small circular icon badge used in list rows/lines and title areas. Circular primaryContainer background with a centered icon.


--- WordHeader.kt ---

# word header

Header component combining word + pronunciation + the pronunciation-play button.

## WordHeader(word, phonetic, onPlayAudio, modifier, playing, centered)

- **word** — word spelling, displayed in large displaySmall-level font
- **phonetic** — IPA pronunciation, nullable; sans-serif, 0.7 opacity
- **onPlayAudio** — callback for playing the pronunciation
- **playing** — whether currently playing
- **centered** — whether to center-align, default true; left-aligned when false


============================================================
app/app/src/main/kotlin/dev/morpho/ui/designsystem/motion/
============================================================


--- Motion.kt ---

# Animation Glossary

The unified entry point for all shared animation behaviors. Each effect reads its duration and easing from the token layer, and automatically degrades to a simple crossfade when the system's "Reduce Motion" mode is enabled.

## rememberSharedAxis()

Call inside a Composable to get a snapshot of the shared axis parameters under the current theme configuration. The returned object is used to drive question transitions and navigation transitions.

## SharedAxis

Holds all parameters required for one shared axis animation. It provides:

- `transform(forward)` — generates a complete set of enter/exit transitions, used in `AnimatedContent` or navigation
- `enter(forward)` / `exit(forward)` — get the enter or exit transition individually

In the forward direction, content slides in from the right and out to the left; the reverse direction is the opposite. In Reduce Motion mode, only a crossfade is performed.

## pressScale(pressed, reducedMotion, scale)

A Modifier extension. When pressed, scales down to 0.97; in Reduce Motion mode, this changes to reducing opacity instead.

## correctSpring()

Spring parameters used for correctness animations (ring expansion, checkmark pop-in). Medium bounce, low stiffness.

## runShake(amplitudePx, durationMs)

Performs horizontal shaking on a floating-point animation value. Keyframe-driven, with the amplitude gradually decaying — it reads as a rejection rather than a shake.

## floatAnimatable(initial)

A convenience factory for creating floating-point animation values, saving you from hand-writing type converters.


============================================================
app/app/src/main/kotlin/dev/morpho/ui/designsystem/theme/
============================================================


--- Color.kt ---

# Brand Palette and Color Schemes

Static brand colors — does not use Material You dynamic color extraction. All colors derive from the four seed colors of the app icon.

## Palette (MorphoPalette)

Four tonal scales, each with multiple steps from dark to light:

- **Ink** — The icon's base color. Text color in light themes, background color in dark themes.
- **Mist** — The color of the static blocks in the icon. Used as a secondary color.
- **Copper** — The color of the active diamond in the icon. The only accent color.
- **Parchment** — The color of the paper sheet in the icon. Background color in light themes, text color in dark themes.

Two additional semantic tonal scales:

- **Verdigris** — Correctness signal, derived from the oxidation color of copper.
- **Oxblood** — Error signal, derived from bookbinding leather.

## Light Scheme (MorphoLightColorScheme)

Parchment for the background, Ink for text, Copper as the focal point. Cards are lighter than the page, as if a sheet of paper resting on a table.

## Dark Scheme (MorphoDarkColorScheme)

Ink for the background, Parchment for text, Copper's role unchanged.

## Semantic Accents (MorphoAccents)

Material roles are overridden with brand semantic colors, accessible via `MorphoTheme.accents`:

- correct / wrong — correct, wrong
- streak — winning streak
- highlight / onHighlight — highlight
- ringTrack — progress ring track
- modePipInactive — inactive state of the mode indicator
- shimmer — shimmer overlay
- motifBase / motifActive / motifMastered — three icon progress states: not started (Mist), learning (Copper), mastered (Ink or Parchment)
- rule — section divider line

Light and dark themes each have a preset set of values (LightAccents / DarkAccents).


--- Theme.kt ---

# YingUse Theme

The entire YingUse theme entry point. Both dark and light schemes are first-class citizens.

## MorphoTheme(darkTheme, tokens, forceReducedMotion, content)

Wraps YingUse content in a Composable. Automatically picks dark/light colors and accent colors based on system settings, injects all design tokens, and detects the system "reduce motion" preference.

- darkTheme — defaults to following the system dark mode
- tokens — replaceable entire design token set (usually uses the default values)
- forceReducedMotion — force overrides the system reduce motion setting; pass `null` to auto-detect

## MorphoTheme Object (Token Accessor)

In any Composable, read tokens via `MorphoTheme.xxx`. Avoid writing raw dp values:

- spacing — spacing tokens
- radii — corner radius tokens
- elevations — shadow elevation tokens
- durations — animation duration tokens
- easings — easing curve tokens
- sizes — fixed component size tokens
- accents — semantic accent colors
- reading — reading typography styles
- reducedMotion — whether the current state is in reduced motion mode


--- Type.kt ---

# Typography System

Two font families serve distinct roles: the brand serif (EB Garamond) is used for UI decoration, while the sans-serif is used for reading content.

## Font Families (MorphoFonts)

- displayFontFamily — EB Garamond, the same font as the icons. Only used for UI decoration such as headings and numbers.
- uiFontFamily — system sans-serif, used for small functional text such as labels and body text.
- readingFontFamily — font for reading content; currently set to a sans-serif (the serif is too thin at body text sizes, unsuitable for extended reading).

## UI Typography (MorphoTypography)

Based on M3 Typography, font families are reassigned: the display / headline / title layers use the brand serif, while the body / label layers use the sans-serif.

## Section Label (MorphoSectionLabel)

A small-caps heading style used above each section on the homepage. Sans-serif with widened letter spacing.

## Reading Typography (MorphoReadingTypography)

A collection of styles for content users actually read; all sans-serif:

- definition — definition
- definitionCaption — single-line definition below image cells
- definitionOption — definition on the three-option mode card (slightly larger)
- sentence — example on the example card
- sentenceCompact — example in the detail page
- etymology — word origin explanation
- phonetic — pronunciation (must be rendered at 0.7 opacity)
- wordHeadline — the word itself in the header and on the detail page
- wordOption — word in quiz options
- spellLetter — letter boxes for spelling input
- statNumber — large numbers on stat tiles and progress rings (serif)
- heroNumber — key numbers on the homepage, such as the count of words already learned (serif)

## PHONETIC_ALPHA

Opacity constant for pronunciation, value 0.7.


============================================================
app/app/src/main/kotlin/dev/morpho/ui/designsystem/token/
============================================================


--- Durations.kt ---

# Animation Duration Tokens

All animation durations (in milliseconds), via `MorphoTheme.durations`:

- press (100) — scale on option press
- flash (150) — red flash for wrong answers
- correct (250) — correct ring + checkmark
- shake (300) — horizontal shake for wrong answers
- transition (300) — question transition (shared X axis)
- promote (400) — mode promotion indicator animation
- progressRing (600) — home progress ring scan
- celebration (1500) — group completion celebration animation limit
- fade (200) — auxiliary content fade in/out


--- Easings.kt ---

# Easing curve tokens

M3 motion easing curve collection, accessed via `MorphoTheme.easings`:

- standard — standard easing (0.2, 0, 0, 1)
- standardDecelerate — standard deceleration
- standardAccelerate — standard acceleration
- emphasized — emphasized easing
- linear — linear


--- Elevations.kt ---

# Elevation Level Tokens

Accessed through `MorphoTheme.elevations` get. The quiz interface stays flat, only floating UI elements are elevated:

- flat (0) — no shadow
- raised (1) — slightly raised
- card (3) — card
- floating (6) — floating elements
- sheet (8) — bottom sheet panel


--- Radii.kt ---

# Radius Tokens

Accessed via/through `MorphoTheme.radii`. Each numeric token comes with a prebuilt rounded corner shape:

- xs (8dp) / shapeXs — labels, pills, small badges
- sm (12dp) / shapeSm — option cards, input fields
- md (16dp) / shapeMd — cards, image cells
- lg (24dp) / shapeLg — panels, primary buttons
- full (999dp) / shapeFull — fully circular


--- Sizes.kt ---

# Fixed Size Tokens

Component's fixed sizes, accessed via `MorphoTheme.sizes`:

- audioChip (44) / audioChipSmall (36) — audio playback button
- modePip (10) / modePipActive (14) — mode indicator dot
- progressRing (200) / progressRingStroke (14) — homepage progress ring
- groupBarHeight (6) — group progress item height
- quizCellMinHeight (132) — quiz cell minimum height
- quizImageMinBand (96) — minimum height of the image area; below this value, show a scroll hint instead of shrinking the answer
- spellBox (40) / spellBoxTall (52) — spelling letter box
- checkBadge (28) — correct checkmark badge
- optionRingWidth (3) — option ring stroke width
- shakeAmplitude (8) — wrong-answer shake amplitude
- sharedAxisSlide (30) — question transition slide distance
- streakBadge (40) — streak badge
- etymologyChip (36) — word origin label


--- Spacing.kt ---

# Spacing tokens

4dp grid spacing system, accessed via `MorphoTheme.spacing`. The interface forbids using bare dp values:

- none (0) — no spacing
- xxs (4) — internal gaps within components
- xs (8) — between closely related elements
- sm (12) — padding within small components
- md (16) — default screen margin and card padding
- lg (20) — grid gap between quiz cells
- xl (24) — between sections
- xxl (32) — above primary action buttons
- xxxl (48) — large whitespace

Two other derived values:

- minTouchTarget (48) — minimum touch target required for accessibility
- screenGutter — default screen horizontal margin (equals md)


--- Tokens.kt ---

# Token Package

Willall designs token packaging for a whole, injected by MorphoTheme. The interface is accessed via/through shortcut methods such as `MorphoTheme.spacing` and `MorphoTheme.durations`, without directly touching this package.

Contains: spacing, radii, elevations, durations, easings, sizes.


============================================================
app/app/src/main/kotlin/dev/morpho/ui/home/
============================================================


--- ActionCard.kt ---

# rows/lines Dynamic Card

The only card on the home page that the user is required to use, serving as the entry point for learning and review.

## ActionCard(today, hasContent, onStartLearning, onStartReview, modifier)

- today: Today's progress data (new words learned, daily goal, reviewed count, pending review count, correct count, total answer count)
- hasContent: Whether the word database has content to learn. When false, the card only shows an empty-state message.
- onStartLearning: Triggered when the "Start Learning" button is clicked
- onStartReview: Triggered when the "Start Review" button is clicked

### Card Content

1. **Two metric blocks** displayed side by side:
   - Pending review count — highlighted when there are items to review, dimmed when it is 0
   - New learning progress — displayed as "learned / goal"
2. **Progress rows/lines** — uses icon-based progress markers to show the daily goal completion ratio
3. **Status text** — shows a congratulatory message when everything is complete
4. **Buttons** — layout determined by priority:
   - When there are items to review: the review button is the primary button (solid), and the learning button is the secondary button (outlined)
   - When there are no items to review: the learning button becomes the primary button; if all of today's tasks are complete, it is disabled


--- ActivityHeatmap.kt ---

# Activity Heatmap

Similar to GitHub contribution graph's grid, using color intensity to indicate daily learning volume.

## ActivityHeatmap(data, modifier)

- data: heatmap data, containing each cell's date and intensity level (0-4), total column count, maximum activity volume

### Display

On the left is the weekday label column (M, W, F three rows have text, the rest are left blank), and on the right is the color block grid. Each column represents one week (7 rows), scrollable horizontally.

Color intensity increases with intensity level: level 0 is the blank background color, levels 1-2 use the brand base color (with increasing transparency), levels 3-4 use the brand highlight color (bronze tone).


--- GreetingHeader.kt ---

# Greeting Bar

The page header area at the top of the homepage.

## `GreetingHeader(greetingPeriod, streakDays, onOpenSettings, modifier)`

- `greetingPeriod`: Time period (morning / afternoon / evening), determines the greeting text
- `streakDays`: Number of consecutive learning days; when greater than 0, a streak badge is shown on the right
- `onOpenSettings`: Triggered when the settings gear button is clicked

### Layout

Left side, arranged vertically: uppercase time-period greeting, app title (large font), brand mark pattern.

Right side, arranged horizontally: streak badge (optional) + settings button.


--- HomeScreen.kt ---

# Home

use's main screen, vertical scrolling layout, summarizing when's daily status and providing learning/review entry points.

## HomeScreen(container, startup, onStartLearning, onStartReview, onOpenSettings, modifier)

Displays the full home page. Automatically refreshes data when opened.

- container: use's dependency container, used to create ViewModel and play click sound effects
- startup: start report, not directly used by this component (consumed by child components)
- onStartLearning: triggered when clicking "Start Learning"
- onStartReview: triggered when clicking "Start Review"
- onOpenSettings: triggered when clicking the settings button

The page is arranged from top to bottom:

1. **Greeting bar** — time-based greeting + streak badge + settings entry point
2. **rows/lines action card** — today's progress and learning/review buttons
3. **This week's activity** — bar chart or heatmap (switchable), with three statistic tiles below (today's word count, accuracy rate, in-progress word count in rows/lines)
4. **Journey progress** — overall completion and remaining days estimate
5. Bottom decorative pattern

The chart style can be toggled between bar chart and heatmap; the toggle button is on the right side of the "This Week" section title.

## formatCount(value)

Formats an integer as a thousand-separated string (e.g., 5500 → "5,500"). Can only be used within this file.


--- HomeViewModel.kt ---

# Home ViewModel

## Exposed status (HomeUiState)

- **loading** — whether currently loading
- **overall** — overall progress: total word count, learned word count, mastered word count
- **today** — today's progress: new words today, daily goal, reviews today, pending review count
- **streakDays** — consecutive learning days
- **contentVersion** — current content version number
- **greetingPeriod** — greeting period (morning/afternoon/evening)
- **weeklyActivity** — recent daily activity data, used for activity charts
- **heatmapData** — heatmap data
- **estimatedDaysRemaining** — estimated remaining days based on recent pace, empty when data is insufficient
- **activityChartStyle** — activity chart style (bar chart/heatmap)
- **hasContent** — whether there is content to learn (total word count greater than zero)

## Accepted operations

### refresh()
Pull all data from the repository database and refresh the entire home page status.

### setActivityChartStyle(style)
Switch the activity chart style and persist it to settings.


--- JourneyProgress.kt ---

# Journey Progress

Displays the overall learning progress of the entire word database, presented in card form.

## JourneyProgress(overall, estimatedDaysRemaining, modifier)

- overall: Overall progress data (total word count in word database, already learned word count, word count in progress rows/lines)
- estimatedDaysRemaining: Estimated remaining days based on current speed, nullable

### Card Content

1. **Section title**
2. **Main number** — already learned word count, displayed in large serif font, with the total word count as the denominator alongside
3. **Milestone track** — uses brand diamond marks to indicate quarter milestones; passed nodes are solid copper, unpassed ones are hollow outlined
4. **Bottom info rows/lines** — shows completion percentage on the left, and on the right displays:
   - A completion mark when everything has been learned
   - "N days remaining" when an estimated value exists
   - Nothing otherwise


--- WeeklyBarChart.kt ---

# Weekly Bar Chart

Displays the learning volume of the last seven days as a bar chart.

## WeeklyBarChart(activity, modifier)

- activity: list of daily activity data, showing the first 7 items. Each item contains the date, total learning word count, number of new words learned, and number of reviews

### Display

The upper area is for the bar chart region, and the lower area is for the day-of-week labels (M T W T F S S).

Bar height is scaled proportionally based on the week's maximum value, and bars with zero value still display a minimum height. The bar and label for the current day use the brand highlight color (bronze), while the rest use a muted base color. The label for the current day uses bold text.


============================================================
app/app/src/main/kotlin/dev/morpho/ui/learn/
============================================================


--- LearnScreen.kt ---

# Learning Page

The three learning modes share one screen. The stimulus area and options area switch according to the current question's mode. A shared X-axis transition animation is used between questions.

## LearnScreen(container, onFinished, onExit, modifier)

Starts a learning session. Upon entry, automatically plans the question set and loads the first question.

- container: should use a dependency container
- onFinished: triggered when the entire question set is completed
- onExit: triggered when the user clicks the close button or actively exits

### Screen Structure

The top app bar shows the title, current round (N / M), mode indicator dots, and a close button.

The main area displays different content based on status:

- **Loading** — centered loading animation
- **No learnable content** — prompt text + return home button
- **Daily goal achieved** — pops up a dialog: optionally "study another set" or "return home". A set is a fixed-size vocabulary unit, not another whole day's batch
- **Normal answering** — stimulus area at the top, options area at the bottom

### Three Modes

1. **Sentence + image** — plays example audio, displays example card (target word highlighted), four-choice image grid
2. **Word + image definition** — plays word audio, displays word and pronunciation, four-choice image + definition grid
3. **Word + text definition** — plays word audio, displays word and pronunciation, four-choice text-only definition list

The corresponding audio plays automatically when each question appears.

### Incorrect Handling

After a wrong selection, a help card is shown: word, pronunciation, and all definitions. The user must re-select the correct answer to continue. The help card appears between the stimulus area and the options area, and the options area does not shift as a result.

### Details Bottom Sheet

After a correct answer, a details panel can pop up showing the word's complete information. Click "Continue" to close.


--- LearnSessionPlanner.kt ---

# Learning Session Pre-Planning

Pure decision logic before the session starts: how many new words can still be learned today, and how much quota "one more group" should give. No clock is held; the date boundary is naturally determined by the caller's current date and stats rows/lines, and automatically resets at local midnight.

## quotaFor(dailyGoal, doneToday) -> quota or empty

Calculates the new word quota for this session start. If today's already learned amount has not reached the goal, return the remaining amount; if the goal has already been reached, return empty, indicating that the caller should first ask the user whether to continue, rather than silently sending another batch.

## extraGroupQuota(config) -> quota

The quota used when the user chooses to continue on the "goal already reached" prompt, equal to the maximum capacity of one unit.

## singleUnit(plan) -> trimmed plan

Takes the plan built with extraGroupQuota and trims it to keep only the first unit. "One more group" means one group; the rest is left for tomorrow.

## emptyState() -> LearnUiState

Returns the UI state when there is no learnable content. It is not counted as "complete" — complete would release session results and navigate to the summary page; an empty plan has no results to show.

## goalReachedState() -> LearnUiState

Returns the UI state when the goal has already been reached. Likewise, it is not "complete", but simply waits for the user's decision.


--- LearnViewModel.kt ---

# learning session ViewModel

Drives a learning session: decides what questions to present, loads content, processes answers, writes progress, emits sound effects and haptic feedback signals. Creates it and automatically starts the session.

## Exposed status (LearnUiState)

- **loading** — currently loading
- **finished** — session has ended, UI should navigate to the summary page
- **empty** — no learnable content
- **goalReached** — today's goal already reached, waiting for the user to decide whether to continue
- **question** — current question (see QuestionUi), empty when no question is available
- **selectedIndex** — index of the user-selected option
- **revealed** — answer already revealed
- **mustRetry** — must select the correct option after a wrong answer before continuing
- **feedback** — current feedback signal (correct/wrong/level-up/group complete/none)
- **detail** — word detail page data, shows detail overlay when non-empty
- **detailContinueLabel** — label text for the detail page continue button
- **round / roundTotal** — current round / total rounds
- **unitIndex / unitCount** — current unit index / total units
- **groupSegments** — progress status of each word in the current group (already passed / completed / in progress / pending / brought in)
- **nowPlayingFile** — name of the currently playing audio file

## QuestionUi

Complete display data for a question: word ID, question mode, word text, pronunciation, word audio, example and highlighted range, example audio, image option list, text option list, correct option index, all senses of the word (used for the wrong-answer hint card).

## Accepted operations

### onOptionSelected(index)
User clicks an option. On a wrong answer, plays error sound and haptics, enters must-retry status; on a correct answer, plays correct/level-up sound, shows the detail page or group completion celebration as appropriate, then advances to the next question. Writes progress in real time on every answer.

### onLearnOneMoreGroup()
Chooses to learn another group on the "goal already reached" prompt. Builds a new session containing only a single unit.

### onDetailDismissed()
Closes the word detail overlay and continues advancing the session.

### onCelebrationFinished()
Called after the group completion celebration animation ends, advances to the next unit.

### onPlayAudio(file)
Manually plays the specified audio file.

### onExit()
Exits the session midway and stops audio.


============================================================
app/app/src/main/kotlin/dev/morpho/ui/review/
============================================================


--- ReviewScreen.kt ---

# Review Screen

Use the "word + imagedefinition" mode uniformly to review all due cards.

## ReviewScreen(container, onFinished, onExit, modifier)

Starts a review session.

- container: dependency container to use
- onFinished: triggered after all due cards are reviewed
- onExit: triggered when the user clicks the close button to exit

### Screen structure

The top app bar shows the title, progress (N / M), and close button.

The main content area displays different content based on status:

- **Loading** — centered loading animation
- **No due cards** — centered hint text
- **Normal answering** — linear progress items + word/pronunciation stimulus area + 2x2 imagedefinition options grid

### Incorrect handling

Same as the learning page: after a wrong selection, a helper card (word, pronunciation, all definitions) appears between the stimulus area and the options area; the user must select the correct answer to continue.

### Details bottom sheet

After a correct answer, a details panel can be shown; click "Continue" to close.

### Answer feedback

On correct/wrong, show a full-screen feedback overlay animation.


--- ReviewViewModel.kt ---

# Review Session ViewModel

Drives a single review session. All questions uniformly use an image + definition grid (dual visual mode). FSRS determines which cards are due; each answer directly updates the card's scheduling parameters. Creating one automatically starts the session.

## Exposed State (ReviewUiState)

- **loading** — currently loading
- **finished** — session has ended; UI should navigate to the summary page
- **empty** — no due review cards
- **question** — current question (see ReviewQuestionUi), null when there are no questions
- **index / total** — current question number / total number of questions
- **selectedIndex** — index of the user's selected option
- **revealed** — answer already revealed
- **mustRetry** — after a wrong answer, must select the correct one to continue
- **feedback** — current feedback signal (correct/error/none)
- **detail** — word detail page data; when non-empty, a detail overlay should appear
- **nowPlayingFile** — the audio file name currently being played

## ReviewQuestionUi

Display data for a review question: word ID, word text, pronunciation, word audio, list of image options, index of the correct option, and all senses of the word.

## Accepted Actions

### onOptionSelected(index)
User clicks an option. On a wrong answer: play error sound and haptics; the first wrong answer immediately writes a failed score to the FSRS card and enters must-retry status. On a correct answer: play the correct sound; the first correct answer writes a success score. After a wrong answer, selecting the correct one pops up the detail overlay; a directly correct answer automatically advances.

### onDetailDismissed()
Closes the detail overlay and advances to the next question.

### onPlayAudio(file)
Manually plays the specified audio file.

### onReplayWord()
Replays the pronunciation of the current question's word.

### onExit()
Exits midway, stopping the audio.


============================================================
app/app/src/main/kotlin/dev/morpho/ui/settings/
============================================================


--- SettingsScreen.kt ---

# Settings Page

The complete list of app settings, divided into five sections and scrolled vertically.

## SettingsScreen(container, startup, onBack, modifier)

- `container`: app dependency container
- `startup`: startup report, providing the total number of words in the database and the content version number
- `onBack`: triggered when the back arrow is clicked

### Section 1: Learning

- **Daily goal** — slider adjustment, with minimum, maximum, and step constraints

### Section 2: Display

- **Theme** — three-option chip group: follow system / light / dark
- **Activity chart style** — two-option chip group: bar chart / heatmap

### Section 3: Feedback

- **Sound effects switch** — when enabled, an additional volume slider is shown
- **Haptic feedback switch**
- **Reduce motion switch**

### Section 4: Data

- **Export progress** — saves a backup file via the system file picker
- **Import progress** — selects a backup file via the system file picker, then shows a confirmation dialog before performing the row replacement. The dialog explicitly tells the user that current data will be overwritten and notes that a restart is required after import. The backup file goes through an integrity check; if the format is invalid, the specific rejection reason is shown.
- Operation status is displayed below the button (export success with file size, export/import failure, etc.)

### Section 5: About

- Total number of words in the database
- Content version number
- App version number


--- SettingsViewModel.kt ---

# Settings ViewModel

All setting changes take effect immediately: the corresponding manager is updated in the same call as persistence, and toggle-type settings also play an effect once so the user perceives it right away.

## Exposed State

### settings
A live stream of user settings, sharing the same StateFlow as SettingsRepository.

### backup (BackupUiState)
UI status of backup/restore operations:
- **busy** — currently executing import/export
- **pending** — a backup file has been staged and awaits confirmation
- **message** — operation result message (export success with byte count / export failure / file rejected with reason / import failure)

## Accepted Operations

### setDailyGoal(value)
Sets the daily new-word goal, automatically aligned to a multiple of the step size.

### setSoundEnabled(value)
Toggles sound effects. Plays a confirmation sound when turned on.

### setSfxVolume(value)
Adjusts the SFX volume. Plays a soft tap sound while adjusting for preview.

### setHapticsEnabled(value)
Toggles haptic feedback. Triggers a level-up vibration when turned on.

### setReducedMotion(value)
Toggles reduced motion. A null value means follow the system.

### setActivityChartStyle(value)
Switches the activity chart style.

### setThemeMode(value)
Switches the theme (light/dark/follow system). The app redraws immediately upon switching.

### suggestedExportName() -> file name
Returns the suggested export file name.

### onExportTarget(uri)
Called after the user selects an export target; performs the export. Plays a completion sound on success.

### onImportSource(uri)
Called after the user selects an import file. It only stages and validates, without writing immediately; once validation passes, pending becomes non-null, awaiting confirmation.

### cancelImport()
Cancels the pending import and cleans up the staged file.

### confirmImport()
Confirms the import, replaces the database, and restarts the app. On success the app restarts directly, with no further status.

### dismissMessage()
Dismisses the operation result message.


============================================================
app/app/src/main/kotlin/dev/morpho/ui/summary/
============================================================


--- SessionSummaryScreen.kt ---

# Session Summary Page

Settlement screen shown after a learning/review session ends, displaying the results of this session.

## SessionSummaryScreen(container, kind, onBackHome, onKeepGoing, modifier)

- container: dependency container to use; provides the result of this session
- kind: session type (learning / review)
- onBackHome: triggered when the "back to home" button is clicked
- onKeepGoing: triggered when the "continue learning" button is clicked

### Display Content

The page is centered with the following arranged vertically:

1. **Title**
2. **Score card** — shows a different title depending on the session type
   - Learning mode: shows the count of newly learned words (with emphasis styling) + accuracy rate
   - Review mode: shows the count of reviewed words (with emphasis styling) + accuracy rate
   - Below the card, a line of text summarizes the results of this session
3. **Streak badge** — only shown when the streak is greater than 0
4. **Back to home button** — always shown
5. **Continue learning button** — only shown in learning mode and when the daily goal has not yet been reached

If no session result is available, fall back to zero values.
