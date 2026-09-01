# admin-ui

63 specs.


============================================================
admin-ui/
============================================================


--- vite.config.ts ---

# Vite build configuration

In development mode, MSW mock is enabled by default (VITE_API_MOCK=1). When set to 0, requests go through the reverse proxy to morphod.

In the proxy configuration, the `/api/stream` SSE endpoint is handled separately: disable response buffering, flush headers immediately, and proactively close the client connection when the upstream disconnects. Other `/api` requests go through the regular proxy.

Test configuration uses the jsdom environment, loads `src/test/setup.ts`, and only matches `*.test.*` and `*.spec.*` files.

Build output directory is dist, with sourcemaps enabled.


============================================================
admin-ui/src/
============================================================


--- main.tsx ---

# App entry point

The mount point of the React app. Assemble in order: QueryClientProvider → LiveStreamProvider → ThemeProvider → RouterProvider.

In development environment (`VITE_API_MOCK=1`), start the MSW service worker first, and only mount the React tree after awaiting interception readiness, to avoid missing initial requests.

Also declare the TanStack Router's Register module augmentation to enable global route type inference.


--- vite-env.d.ts ---

Vite environment variables' type declaration. Defines three optional variables: VITE_API_MOCK, VITE_MORPHOD_URL, VITE_MORPHO_USER.


============================================================
admin-ui/src/api/
============================================================


--- changeStream.ts ---

# SSE change stream transport

Manages a persistent EventSource connection to `GET /api/stream`. Morphod emits `event: change` frames with `{entity_type, entity_ids}` payloads, coalesced server-side over 250 ms. Handles reconnection with exponential backoff and jitter, and a liveness watchdog that re-opens after 90 s of silence (three missed ping intervals).

## subscribeToChanges(options) -> unsubscribe

Opens the stream and keeps it alive indefinitely. Returns a teardown function that closes the socket and cancels pending reconnects. The `onChange` callback fires for each decoded `ChangeEvent`; `onStatus` reports lifecycle transitions (`connecting`, `open`, `reconnecting`, `closed`). Inject `createSource` and `random` for deterministic testing.

## reconnectDelay(attempt, random?) -> number

Exponential backoff with full jitter on the upper half of the window. First retry ~500-1000 ms, ceiling 30 s.

## parseChangeFrame(data) -> ChangeEvent | null

Decodes one SSE `data:` payload. Returns null for anything that is not a well-formed `ChangeEvent`.

## Constraints
- Do not use the browser's built-in EventSource reconnect; this module drives its own schedule.
- The idle watchdog is the only mechanism that detects a stale connection held open by intermediaries.


--- client.ts ---

# API client

The single fetch wrapper for every `/api` call in admin-ui. Sends/receives JSON, attaches the `X-Morpho-User` header on mutations, and converts non-2xx responses into typed `ApiError` instances carrying the decoded error envelope.

## request<T>(path, options?) -> Promise<T>

Builds the URL from `API_BASE + path + query`, sends the request, and returns the decoded body. Throws `ApiError` on any non-2xx status. Supports JSON body, FormData, and AbortSignal.

## ApiError

Typed error for every failed response. Properties: `status`, `code`, `body`. Convenience getters: `isConflict` (409), `isNotFound` (404).

## extractGateFailures(error) -> ExportGateFailure[]

Pulls the gate-failure array from an export 409 response, tolerating both top-level `failures` and `error.details.failures` shapes.

## mediaUrl(fileHash) -> string

Absolute URL for a content-addressed media file: `/api/media/{file_hash}`.

## buildQuery(params?) -> string

Encodes a flat record into a query string, dropping null/undefined/empty values.

## Constraints
- Nothing else in admin-ui may call `fetch` directly; all server communication goes through `request()`.
- The `MORPHO_USER` identity comes from `VITE_MORPHO_USER` env var, defaulting to `"local"`.


--- endpoints.ts ---

# API endpoint functions

One async function per row of `docs/contracts/admin-api.md`. Every response passes through `mappers.ts` before reaching the UI layer. All word-scoped mutations return the refreshed `WordDetail` because a single write cascades through selection, approval, readiness, and blockers.

## Dashboard & observability
- `getDashboard(signal?)` -- GET /dashboard
- `getEvents(query?, signal?)` -- GET /events, paginated audit log newest-first
- `getJobs(signal?)` -- GET /jobs, live queue snapshot

## Words
- `listWords(query?, signal?)` -- GET /words, paginated with role/ready/blocker/group/q filters
- `listGallery(query?, signal?)` -- GET /gallery, selected-image overview for visual review
- `getWord(wordId, signal?)` -- GET /words/{id}, full detail with slots
- `createWord(body)` -- POST /words

## Candidates & selections
- `mintDefinitionCandidate(body)` / `mintExampleCandidate(body)` / `uploadImageCandidate({word_id, file})` -- POST /candidates/{kind}
- `rejectCandidate(kind, candId)` -- POST /candidates/{kind}/{cand_id}/reject
- `overrideSelection(kind, body)` -- POST /selections/{kind}
- `approveSelection(kind, body)` / `unapproveSelection(kind, body)` -- POST/DELETE /selections/{kind}/approve
- `setPrimarySense(body)` -- POST /selections/definition/primary
- `setSenseEnabled(body)` -- POST /selections/definition/enabled

## OOV queue
- `listOov(query?, signal?)` -- GET /oov, paginated
- `resolveOov(lemma, body)` -- POST /oov/{lemma}/resolve

## Dead letters
- `listDeadLetters(query?, signal?)` -- GET /dead-letters, paginated
- `retryDeadLetter(body)` / `waiveDeadLetter(body)` -- POST /dead-letters/retry|waive

## Plan & releases
- `getPlan(signal?)` -- GET /plan
- `getPlanGroup(seq, signal?)` -- GET /plan/groups/{seq}
- `listReleases(signal?)` -- GET /releases, paginated
- `getReleasePreview(signal?)` -- GET /releases/preview, holdback report
- `exportRelease(body?)` -- POST /releases/export, 409 with gate failures on validation fail

## Constraints
- Do not call `fetch` or `request()` from outside this file; all API access goes through these functions.
- Every mutation returns updated domain objects, not raw wire data.


--- index.ts ---

API layer's unified export entry point. It re-exports types, client, endpoints, queryKeys, and mappers that contain commonly used conversion functions.


--- mappers.ts ---

# Wire-to-domain mappers

Pure functions that normalize morphod's wire format into typed domain objects. Handles SQLite booleans (0/1), JSON-encoded TEXT columns, and nullable fields. Unit-tested core of the API client -- everything above this layer sees real booleans and parsed objects.

Each `map*` function takes `unknown` and returns a fully-typed domain object with safe fallbacks. The module exports mappers for every entity: `mapWord`, `mapWordListItem`, `mapWordDetail`, `mapGalleryItem`, `mapEvent`, `mapDashboard`, `mapJobsSnapshot`, `mapOovEntry`, `mapDeadLetter`, `mapPlanSummary`, `mapPlanGroupDetail`, `mapRelease`, `mapHoldbackReport`, and `mapPaginated`.

## Utility exports
- `toBool(value)` -- coerces 0/1/string/boolean to boolean
- `toNumber(value, fallback?)` / `toNullableNumber(value)` -- safe numeric coercion
- `toString(value, fallback?)` / `toNullableString(value)` -- safe string coercion
- `parseJsonColumn<T>(value, fallback)` -- parses a TEXT column holding JSON; passes through already-decoded values
- `toBlockers(value)` -- parses the `words.blockers` JSON array

## Constraints
- Pure functions only; no side effects, no imports from outside `./types`.
- Do not skip mapping for any field -- wire shapes are not stable enough to pass through.


--- queryKeys.ts ---

# React Query key factory

Central `qk` object producing structured query keys for every API entity. Keys are hierarchical so `invalidateQueries({ queryKey: qk.words() })` sweeps every filtered word list in one call. Also exports `wordMutationInvalidations` -- the set of key families any word-scoped mutation should invalidate (words, dashboard, oov, releases, plan).


--- types.ts ---

# Admin API type definitions

Single source of truth for every server-facing type in admin-ui. Mirrors `admin-api.md` (shapes) and `working-db.sql` (field names, enums). All types are the domain form -- booleans are `boolean`, JSON blobs are parsed objects, timestamps are UTC ISO-8601 strings. Exports enums (`WordRole`, `Pos`, `AssetKind`, `BlockerCode`, ...), entity interfaces (`Word`, `WordDetail`, `GalleryItem`, `AdminEvent`, `PlanSummary`, `Release`, `HoldbackReport`, ...), query/body interfaces for every endpoint, and envelope types (`Paginated<T>`, `ApiErrorEnvelope`).

## Constraints
- Nothing else in admin-ui may invent a server-facing field; all additions go here.
- Blocker codes are an open union -- core may add codes, so unknown strings must stay renderable.


============================================================
admin-ui/src/app/
============================================================


--- liveStream.tsx ---

All should use the single SSE change stream Provider, manage the singleton connection lifecycle, and inject status downstream.

## export

- **LiveStreamProvider** — React component that wraps the child component tree. Internally calls `useChangeStream()` to establish the singleton SSE connection, and passes the connection status down through `LiveStreamContext`. Only one instance should exist in the app, placed at the top of the component tree.

## constraint

- Do not create multiple `LiveStreamProvider` instances. A second instance opens a second EventSource connection, causing duplicate invalidation refreshes on every change and consuming an extra browser concurrent connection slot (limited to six).
- This component does not accept configuration props — the SSE address and batching strategy are determined internally by `useChangeStream`.


--- liveStreamContext.ts ---

SSE change stream health status context: whether local read connections are alive.

## export

- **LiveStreamContext** — React context that carries the change stream connection status (connection status enum, whether it is enabled). Do not read via `useContext` directly; use the hook below.
- **useLiveStream()** — hook for reading the current change stream status. Returns `{ status, enabled }`. `status` is one of `'open'` | `'connecting'` | `'reconnecting'` | `'closed'`; `enabled` is `false` in mock mode.
- **DISABLED_STREAM** — sentinel constant indicating that the stream is closed/unusable (`status: 'closed', enabled: false`). Used as the default value of the context, and can also be passed directly in tests.

## constraint

- Do not use `useContext(LiveStreamContext)` directly in components; always use the `useLiveStream()` hook.
- This file only defines the context and read interface; it does not manage the SSE connection lifecycle — the connection is managed in the `liveStream.tsx` Provider.


--- queryClient.ts ---

React Query client factory, providing a uniformly configured QueryClient instance.

## export

- **createQueryClient()** — Creates and returns a configured QueryClient. Default configuration: query data is marked as stale after 15 seconds; garbage collection time is 5 minutes; no automatic refetch on window focus; 4xx errors (explicitly rejected by the service) are not retried, 5xx errors are retried at most 2 times; all mutations are not retried.

## constraint

- Do not bypass this factory by manually calling `new QueryClient()`, otherwise the retry policy and stale time will be inconsistent, leading to issues such as infinite retries for 4xx errors.
- Each entry point should be called only once, and the returned instance should be passed to `QueryClientProvider`.


--- theme.tsx ---

Ant Design theme management, providing light/dark mode switching and persistence to localStorage.

## exports

- **ThemeProvider** — a theme provider component that wraps the entire application. Internally, it configures Ant Design's ConfigProvider (including tokens such as brand color, border radius, and font), switches between light/dark algorithms based on the current mode, and also sets the `data-theme` and `color-scheme` properties on `<html>`.
- **useThemeMode()** — a hook for reading and controlling the theme mode, returning `{ mode, toggle, setMode }`. `mode` is the currently active `'light'` or `'dark'`; `toggle()` switches between the two; `setMode()` directly specifies the mode.
- **ThemeMode** — a type whose value is `"light"` or `"dark"`.

## constraints

- Do not use `useThemeMode()` outside of ThemeProvider; otherwise you'll get empty default values.
- Initial mode determination logic: first read localStorage (key: `morpho.admin.theme`); if there's no value, follow the system's `prefers-color-scheme`. Do not implement any separate initialization logic.
- There is no `"system"` mode here — the system preference only takes effect on first load when localStorage has no value. After that, it is always stored as `"light"` or `"dark"`.


============================================================
admin-ui/src/components/
============================================================


--- AppLayout.tsx ---

Overall shell layout, providing sidebar navigation, top bar tools, and main content area.

## Export

- **AppLayout** — receives `children` for rendering the main content area. The sidebar contains seven navigation items (Dashboard, Words, Gallery, OOV Queue, Dead Letters, Plan, Releases), where OOV Queue and Dead Letters display count badges next to them. The top bar, from left to right, consists of: collapse/expand sidebar button, global search box (GlobalSearch), change stream status indicator (live/connecting/reconnecting/offline), data source label (mock data or live morphod), and light/dark theme toggle button. The sidebar is collapsible; when collapsed, badges and brand text are hidden.

## Constraints

- Don't wrap another Layout outside AppLayout; it is already a full-screen layout with `minHeight: 100vh`.
- Don't manually pass in navigation data; the navigation items and badge values are fetched internally by the component from the `useDashboard` query.


--- AudioButton.tsx ---

# Play Audio Button by Content Hash

## Export

- **AudioButton** — Click to play the corresponding audio via `/api/media/{hash}`. Accepts the following properties: `fileHash` (audio file hash; when null, the button is disabled and prompts that there is no audio), `disabled` (force disable), `title` (custom tooltip text), `size` (button size, default small). While playing, the icon changes to a pause icon; clicking again stops playback and returns to the beginning. A spinning icon is shown during loading.

## Constraints

- When `fileHash` is null or `disabled` is true, the button is automatically disabled — no need to wrap it with additional disabled logic externally.
- Each button instance manages its own `<audio>` element independently. Do not frequently mount/unmount this component in the same location, as doing so will repeatedly create audio objects.


--- EventTimeline.tsx ---

Renders an array of admin events into a color-coded Ant Design Timeline.

## Export

- **EventTimeline** — receives properties: `events` (array of AdminEvent), `emptyText` (optional, placeholder text when the list is empty). Each event displays: an action-type label (colored by action — approved green, candidate_rejected red, plan_rebuilt cyan, etc.), an actor label (admin: purple prefix, worker: blue prefix), the entity type and ID, and a relative time (hover to show the full timestamp). If the event carries a `detail` object, its first four key-value pairs are displayed below in monospace font. When the event list is empty, an empty-state placeholder is shown.

## Constraints

- The passed-in `events` array is not sorted; it is rendered in its original order — sorting should be handled in the query layer.
- Do not pass objects of types other than `AdminEvent`; the component depends on the fields `action`, `actor`, `entity_type`, `entity_id`, `ts`, and `detail`.


--- GlobalSearch.tsx ---

Global search box in the top bar, supports keyboard shortcut focusing and auto-complete navigation.

## export

- **GlobalSearch** — No property required. After entering text, debounce for 200ms before querying the backend for matching words (max 8 items). The dropdown list displays lemma, role tags, and ready status. Selecting an item, or pressing Enter to select the first item, automatically navigates to that word's detail page (`/words/$wordId`). Pressing the `/` key anywhere on the page focuses the search box (pressing `/` inside input fields, textareas, and editable elements will not trigger this).

## constraint

- This component is already embedded in the top bar of AppLayout; do not reuse it elsewhere.
- The search box's DOM id is `morpho-global-search`; do not create an element with the same id on the page.


--- HighlightText.tsx ---

# Text highlighting components

Two components for inline text highlighting. `HighlightRange` wraps a `[start, end)` byte range in a highlight span (used for example sentences with keyword offsets). `HighlightToken` highlights every whole-word occurrence of a token (used for OOV lemma highlighting in definitions). Both fall back to plain text on invalid inputs.


--- QueryState.tsx ---

# Query state wrapper

Generic component that renders the loading / error / empty / data states for any `UseQueryResult`. Provides a consistent skeleton, a typed error result with retry button (404 gets its own treatment), and an optional empty-state branch. Wrap any data-dependent section in `<QueryState query={q}>{(data) => ...}</QueryState>` to avoid repeating the three-branch pattern on every page.


--- StatusChips.tsx ---

# Status chip library

Reusable visual indicators used across all admin-ui tables and detail views.

- `ReadyBadge` -- green "ready" or amber "blocked" tag
- `BlockerTags` -- renders up to `max` blocker codes as color-coded tooltipped tags, with a "+N" overflow
- `AssetChip` -- single ready/partial/missing/failed chip with icon
- `WordAssetChips` -- rolls a `WordListItem` row into four asset chips (def, ex, img, tts) derived from blockers
- `SourceBadge` -- color-coded tag for candidate source (freedict, llm, unsplash, sdxl, etc.)
- `ScoreBadge` -- auto-score with tooltip showing score_detail breakdown
- `RoleTag` -- word role with aux_status suffix
- `PinnedMark` / `PrimaryMark` / `ApprovalTag` -- selection metadata indicators


--- WordLink.tsx ---

Deep link to the word detail page. Accepts `wordId`, optional `tab` to open a specific detail tab, and optional `strong` for bold styling. Uses the router's `<Link>` directly (not nested inside `<Typography.Link>`) to avoid invalid anchor nesting.


============================================================
admin-ui/src/features/dashboard/
============================================================


--- DashboardPage.tsx ---

All-board dashboard page, displaying word database overall progress and resource gap overview.

## export

- **DashboardPage** — A page component with no parameters. Renders three stat cards at the top (total word count, ready word count, blocked word count); a ready-rate ring chart in the middle; four groups of bar charts side by side below, showing the gap counts for definition, example, image, and TTS resources respectively; and an event timeline at the bottom, listing recent system events in reverse chronological order. It can be directly mounted as a route page.

## constraint

- Accepts no props; all data is fetched from the backend API; don't attempt to pass through external status.
- Don't manually refresh dashboard data outside this component—the component internally manages polling and refresh logic on its own.


============================================================
admin-ui/src/features/deadletters/
============================================================


--- DeadLettersPage.tsx ---

Dead letter queue page, view and handle all failed background tasks.

## export

- **DeadLettersPage** — A page component with no parameters. Renders a paginated table grouped by rate channels, with each row displaying the details of a failed task. Each row provides two action buttons: "retry" re-enqueues the task for execution; "abandon" marks the task as already handled and removes it from the queue.

## constraints

- Does not accept props; used directly as a route page.
- The "abandon" operation is irreversible; the task will be permanently marked as already handled.
- Do not poll the dead letter list externally — pagination and refresh logic is managed internally by the component.


============================================================
admin-ui/src/features/gallery/
============================================================


--- GalleryPage.tsx ---

Image gallery page, supports browsing, filtering, and triaging word database images.

## Export

- **GalleryPage** — Receives `GalleryPageProps`. Provides two display modes: grid and review, supporting infinite scroll loading. Can filter by image source and review status, and sort by CLIP similarity. When switching to the flagged or needs_regen triage view, displays the local mark list managed by `useImageFlags` (data stored in localStorage).
- **GalleryPageProps** — Contains the current search status object and search change callback functions. The parent component controls the gallery's filter conditions through these props.
- **GallerySearch** — Search parameter structure, fields include: source (image source), approved (review status), q (keyword), sort (sorting method), view (view mode).
- **GalleryViewMode** — View mode, takes values `"flagged"` or `"needs_regen"`, used for switching triage views.

## Constraints

- Search status is owned by the parent component and passed in via props; do not create an independent search status inside GalleryPage.
- Mark data is stored in localStorage, valid only in the current browser, and not synced to the backend.
- The view field in GallerySearch only accepts values defined by GalleryViewMode; it cannot be arbitrarily extended.


--- useImageFlags.ts ---

# Gallery image triage flags

Client-only localStorage hook for the gallery's flag/review/resolve workflow. Tracks two sets of word IDs: `flagged` (needs attention) and `needsRegen` (image should be regenerated). State syncs across browser tabs via the `storage` event. Provides `flag`, `unflag`, `markNeedsRegen`, `clear`, boolean checks, and aggregate counts. No server round-trip -- purely a local worklist marker.


============================================================
admin-ui/src/features/oov/
============================================================


--- OovPage.tsx ---

# Out-of-scope queue page

Paginated table of OOV (out-of-vocabulary) lemmas found in selected definitions. Each row expands to show every definition occurrence with the offending token highlighted. Status filter switches between open, rewritten, promoted, and auto-closed views. Two resolution paths via a modal: "Rewrite" mints a new `llm_rewrite` candidate with replacement text and selects it; "Promote" inserts the lemma as an auxiliary word. The rewrite form pre-fills from the engine's LLM draft when available, and validates that the replacement no longer contains the OOV token.


============================================================
admin-ui/src/features/plan/
============================================================


--- PlanPage.tsx ---

# Learning plan page

Displays the current plan artifact: summary statistics (word count, groups, edges, SCC groups, largest group, avg size), artifact metadata with diff-vs-previous, and a two-panel master-detail layout. Left panel is a virtualized group list (windowed rendering for plans with hundreds of groups), right panel shows the selected group's words in learning order with role, readiness, and blocker tags. Group types (scc, root, semantic, fill) are color-coded and tooltipped.


============================================================
admin-ui/src/features/releases/
============================================================


--- ReleasesPage.tsx ---

# Releases page

Two sections: holdback report and release history. The holdback report shows shippable/exportable/excluded counts, validation gate status, and a table of held-back words sorted by downstream impact (fix the top row first). Gate failures are listed when present. The export button opens a modal for triggering `POST /releases/export` with optional notes; on 409 the gate failures are displayed inline. The release history table shows every exported version with plan, word/media counts, size, and notes.


============================================================
admin-ui/src/features/words/
============================================================


--- AudioTab.tsx ---

# TTS audio tab

Word detail tab showing all desired TTS texts for a word. Table columns: play button, slot reference (lemma/sense/example), synthesized text, status (ready/missing/failed), duration, and input_hash with voice/engine tooltip. Distinguishes three states within "missing": genuinely pending (no tts_assets row, no error), errored (has last_error but status is missing), and failed (status=failed). Alerts surface failed/errored syntheses with instructions to retry or waive on the dead letters screen.


--- BulkApproveModal.tsx ---

# Bulk approve modal

Two-phase modal for bulk approval runs. Phase 1: confirmation with word count, action description, and a note that words with nothing to approve are skipped. Phase 2: live progress bar with approved/skipped/failed tallies, current-word indicator, cancel button (stops after the in-flight word), and result lists for failures and skips. Failed words can be retried from the outcome view.


--- DistractorsTab.tsx ---

# Distractors tab

Word detail tab showing the three bound distractors as cards. Each card displays the distractor lemma (linked), rank, core_ready status, blockers, and binding metadata. Alerts when any distractor is not core-ready (the parent word cannot ship until they are). Distractors are bound once and never recomputed automatically -- only a manual rebind changes them.


--- ExamplesTab.tsx ---

# Examples tab

Word detail tab managing three example slots. Each slot shows its current selection (highlighted sentence), a dropdown to pick a different candidate, and approve/unapprove controls. Slot 1 is the mode-1 sentence and a hard factory gate; slots 2-3 are optional. The shared candidate pool is displayed below with source, score, highlight range, and reject buttons. A "mint manual example" form validates that the highlight phrase occurs verbatim in the sentence.


--- ImageTab.tsx ---

# Image tab

Word detail tab for image candidate management. Shows the live image's approval controls, a grid of image candidate cards (thumbnail, source, score, license, dimensions, file_hash) with "use this" and reject buttons, and a drag-and-drop upload zone for manual candidates (8 MB limit, image/* only, content-addressed storage). Exactly one live image per word; rejected images fall back to the next candidate, and when none remain the image lane re-derives.


--- SensesTab.tsx ---

# Senses tab

Word detail tab for definition management. Renders one card per POS slot, each containing: primary-sense marker with make-primary button, enabled/disabled toggle, approve/unapprove controls (disabled when an OOS blocker is present), and the candidate list with select, reject, source, score, rewrite-lineage, and text_hash. A "mint manual definition" form creates a new immutable candidate. Approval is blocked while OOV tokens remain -- an alert directs the operator to the OOV queue.


--- SlotHeader.tsx ---

# Slot approval controls

Shared approve/unapprove widget used by every asset slot (senses, examples, image). Shows the approval tag, pinned mark, provenance line (auto/human, revision, approved-by/at), and an approve or un-approve button. Accepts an optional `disabledReason` tooltip to block approval (used by the senses tab when OOS blockers are present). Renders "Nothing selected in this slot" when selection is null.


--- WordDetailPage.tsx ---

# Word detail page

Top-level page for a single word. Header card shows lemma, phonetic, audio play button, role, readiness badge, blocker tags, word_id, frequency rank, creation metadata, and etymology. Below it, a tab bar switches between Senses, Examples, Image, Audio, Distractors, and Events tabs -- each with a count label (e.g. "Audio (3/5)"). Tab labels are destroyed on hide. The Events tab shows the word's audit timeline with a link to search related words. All mutations go through `useWordMutations(wordId)`.


--- WordsPage.tsx ---

# Words list page

Paginated, filterable word table with row selection and bulk-approve actions. Filters: approval worklist presets (awaiting sense/example/image), lemma search, role, readiness, blocker code, and plan group. Row selection persists across pages and filters; "select all matching" walks every page of the current filter. Selected words can be bulk-approved for primary sense, example slot 1, or image via the `BulkApproveModal`. Clicking a row navigates to the word detail page; clicking the checkbox column selects without navigating.


--- bulkApprove.ts ---

# Bulk approve logic

Pure decision logic for bulk-approving selections across the word list. Three kinds: `definition_primary` (approves the is_primary sense), `example_slot_1` (approves slot 1), `image` (approves the live image).

## resolveApprovalTarget(kind, detail) -> ApprovalTarget

Given a bulk-approve kind and a word's full detail, decides whether to approve (returning the selection kind and key) or skip (with a reason). Skips when there is no selection, the selection is disabled, or it is already approved. Callers never need to inspect the detail themselves.

## BULK_ACTIONS

Readonly array of `BulkApproveAction` descriptors with kind, label, description, and blocker code.

## tally(items) -> BulkApproveTally

Counts approved/skipped/failed/done from a list of `BulkApproveItem` results.


--- tabs.ts ---

Tab key enum and ordered list for the word detail page. `WordDetailTab` is used as a route search param so every tab is deep-linkable.


--- wordsSearch.ts ---

Route search state interface for the word list. Every filter is a URL parameter so any view is a shareable link. All fields optional; page defaults to 1, page_size to 25.


============================================================
admin-ui/src/hooks/
============================================================


--- queries.ts ---

# React Query hooks

Central query and mutation hooks for admin-ui.

## Read hooks
`useDashboard`, `useWordList`, `useWordDetail`, `useOovList`, `useDeadLetters`, `usePlan`, `usePlanGroup`, `useReleases`, `useReleasePreview`, `useEvents`. Dashboard uses fallback polling (30 s) only when the change stream is reconnecting; all others rely on stream-driven invalidation.

## useWordMutations(wordId) -> mutation bundle

All word-scoped operations for one word: `approve`, `unapprove`, `select`, `reject`, `setPrimary`, `setEnabled`, `mintDefinition`, `mintExample`, `uploadImage`. Approve/unapprove use optimistic updates; all mutations invalidate the word detail, word list, dashboard, OOV, releases, plan, and events caches on success. Exposes a `busy` boolean.

## applyApproval(detail, kind, key, approved) -> WordDetail

Pure helper for optimistic approval toggling, unit-testable separately.

## Queue mutations
- `useResolveOov()` -- promote or rewrite an OOV lemma
- `useDeadLetterActions()` -- retry (deletes job_state row) or waive (arms fallback rules)
- `useExportRelease()` -- triggers `POST /releases/export`


--- useBulkApprove.ts ---

# Bulk approve driver

Sequential executor for bulk approval runs. Processes one word at a time (not parallel) because morphod serializes writes behind a single writer task.

## collectMatchingWords(query, signal, onProgress?) -> WordListItem[]

Walks every page of a filter (up to 200 per page, the morphod cap) to collect the full worklist. Abortable; used by "select all matching" on the words page.

## useBulkApprove() -> BulkApproveController

React hook that manages a cancellable run. `start(kind, words)` reads each word's detail, calls `resolveApprovalTarget` to decide approve/skip, fires the approval, and patches progress state. `cancel()` stops after the current word. `reset()` clears the run state. Invalidates all relevant caches once at the end of the run rather than per-word.


--- useChangeStream.ts ---

# Change stream hook

Translates SSE change frames from `GET /api/stream` into targeted React Query invalidations. Maps each `entity_type` to the query families it can affect (e.g. `word` -> words, dashboard, plan, releases, events; `job_state` -> deadLetters, jobs, dashboard). Word-scoped changes also pin the exact detail key.

Client-side coalescing batches frames over a 300 ms window with a 1 s floor between invalidation passes, so a converging fetch run does not turn the change bus into a load generator. On reconnect, all queries are invalidated to catch frames missed while the socket was down.

## Exports
- `CHANGE_TARGETS` -- entity_type to invalidation target mapping
- `targetsFor(entityType)` -- lookup with fallback to dashboard+events
- `collectInvalidations(events)` -- folds a batch into minimal invalidation set
- `applyInvalidations(client, batch)` -- executes the invalidations
- `useChangeStream()` -- hook returning `{status, enabled}`

## Constraints
- Mock mode (`VITE_API_MOCK=1`) keeps the stream disabled; fixtures only change via the console's own mutations.


============================================================
admin-ui/src/lib/
============================================================


--- errors.ts ---

Error message extraction utilities. `errorMessage(error)` returns a human-readable string from `ApiError`, `Error`, or any thrown value. `errorCode(error)` returns the error code from `ApiError` or undefined.


--- format.ts ---

Presentation helpers. `formatTimestamp(iso)` renders a locale-aware short date/time. `relativeTime(iso)` returns "Ns/m/h/d ago". `formatBytes(bytes)` formats as B/KB/MB/GB. Pure functions, no React.


============================================================
admin-ui/src/mocks/
============================================================


--- browser.ts ---

# Browser-side MSW entry point

## startMockWorker() → Promise
Starts the service worker interceptor. The returned Promise only resolves after the worker actually begins intercepting requests, so callers can await it before mounting the React tree. Unmatched requests will be passed through.


--- db.ts ---

# Memory Fixture Database

The MockState is the status mock behind the MSW handler. It mirrors the table structure of working-db.sql, and all behavior is consistent with the real backend: readiness is computed in real time from status (not stored as flag bits), the OOV queue is inferred from the selected definitions, and every write operation appends event rows/lines.

## db() → MockState
Returns the current database status. On first call, it automatically executes seed(); subsequent calls return the same reference.

## resetDb()
Clears all data and re-seeds. Call this after each test to prevent status leakage.

## computeReadiness(wordId) → { ready, blockers }
Computes a word's complete readiness status, including core readiness (definition/example/image/TTS review status) plus blocker readiness, and whether it is in the current plan.

## computeCoreReady(wordId) → { core_ready, blockers }
Only computes core readiness without recursing into blockers. Recursion depth is limited to 1 level.

## computeHoldback() → { shippable, exportable, rows }
Dependency-closure pruning. shippable = the set of words passing all per-word gates; exportable = the maximum subset after fixed-point contraction through the dependency closure; rows = excluded words along with their root causes and blast radius.

## computeGateFailures(exportable) → GateFailure[]
Hard gate checks before export: the export set is non-empty, no unresolved OOS, no dead letter.

## recordEvent(actor, entityType, entityId, action, detail?) → AdminEvent
Inserts one audit event item at the head of the events table.

## Write Operations
rejectCandidate, overrideSelection, setApproval, setPrimarySense, setSenseEnabled, ensureTtsForWord — each write operation triggers its associated cascades (auto re-selection after candidate rejection, auto pinning after approval, auto-filling of ready rows/lines after TTS requirement changes).

## OOV Resolution
resolveOosPromote(lemma, actor) — promotes an OOS word to an auxiliary word.
resolveOosRewrite(lemma, defCandId, text, actor) — replaces the original definition with the rewrite; once the OOS occurrence count drops to zero, the queue item is automatically closed.
syncOosQueue() — cleans up open items that no longer have occurrences for auto_closed.

## Export
insertRelease(exportable, actor, notes) — creates a release row/line and counts media count and byte size.

## Constraints
- This is fixture code. Hashing and word splitting are simplified implementations and cannot be used in production logic.
- MockState is a singleton mutable reference; directly modifying its fields affects all subsequent reads.


--- handlers.ts ---

# MSW request handlers

MSW v2 handlers implementing `admin-api.md` verbatim against the stateful fixture DB. Every endpoint from the contract is covered: dashboard, events, jobs, words (list, detail, create), candidates (mint, reject), selections (override, approve, unapprove, primary, enabled), gallery, OOV queue (list, resolve), dead letters (list, retry, waive), plan (summary, group detail), releases (list, preview, export), media serving, and the SSE stream stub. Response shapes mirror `src/api/types.ts`; a mismatch breaks the typecheck. Mutations update the in-memory DB and recompute readiness/blockers.


--- node.ts ---

Node-side MSW server. Export the server and start it in `beforeAll` in Vitest's setup file. Use the same handlers as the browser side.


============================================================
admin-ui/src/mocks/fixtures/
============================================================


--- lexicon.ts ---

# MSW fixture seed data

Seed lexicon for the mock database. Contains `SEED_WORDS`: real exam vocabulary with plausible definitions, examples, etymologies, and distractor bindings. Each word has a `stage` (ready, pending_approval, oos, no_image, tts_failed, thin, fresh, base) that drives how complete the generated assets are, giving the console a realistic spread of readiness and blockers across target, auxiliary, and base words. Also exports `OOS_LEMMAS` (tokens appearing in definitions that match no word row) and `REWRITE_DRAFTS` (pre-generated LLM rewrites for OOV resolution testing).


--- media.ts ---

# Mock media byte sources

Provides stable binary content for the mocked `/api/media/{file_hash}` endpoint. `silentOggBytes()` returns a real 350 ms silent Ogg/Opus clip (450 bytes) reused for all TTS hashes -- a genuine container so `<audio>` reports duration and fires `ended`. `placeholderImageSvg(fileHash, label)` generates deterministic SVG placeholder artwork from the hash, so each candidate image is visually distinct with a stable hue.


============================================================
admin-ui/src/routes/
============================================================


--- __root.tsx ---

# Root route

TanStack Router's root layout. All child routes render in the Outlet inside AppLayout.

Additionally provides 404 page and global error fallback page, both with "back to home" or "retry" buttons.

Exports RouterContext interface requiring upper layer to inject QueryClient.


--- dead-letters.tsx ---

route `/dead-letters`, renders DeadLettersPage.


--- gallery.tsx ---

Image database route: `/gallery`. URL search parameters include five items: source, review status, keyword, sorting, and view mode. All are passed to GalleryPage after whitelist validation. When parameters change, use `replace` to write back to the URL, without generating browser history items.


--- index.tsx ---

Root route `/`, renders DashboardPage.


--- oov.tsx ---

# OOV route

Route `/oov`. URL search parameters include status, page, page_size. After whitelist validation, pass to OovPage. When parameters change, use replace to write back to the URL.


--- plan.tsx ---

route `/plan`, render PlanPage.


--- releases.tsx ---

route `/releases`, renders ReleasesPage.


============================================================
admin-ui/src/routes/words/
============================================================


--- $wordId.tsx ---

# worditems Details Route

Route `/words/$wordId`. Extract the `wordId` and `tab` parameters from the URL (with tab whitelist validation), and pass them to WordDetailPage. Each tab of each word item can be deep-linked to.


--- index.tsx ---

# word table route

route `/words/`. URL search parameters include role, ready, blocker, group, page, page_size, q. After type validation is performed on all of them, they are passed to WordsPage. Any filtered item conditions should be shareable links.


============================================================
admin-ui/src/test/
============================================================


--- renderWithProviders.tsx ---

# Test Render Helper

## renderPage(ui, initialPath?) → RenderResult & { queryClient, router }

Embeds the component under test into a one-off in-memory router (with stubs registered for all target routes), wrapped externally with QueryClientProvider and ThemeProvider. Returns the testing-library rendering result, along with queryClient and router references, making it easy to manipulate cache and navigation in tests.

QueryClient is configured with no retry and zero GC time, suitable for test environments.


--- setup.ts ---

# Vitest Global Setup

Before all tests, start the MSW Node server (unmatched requests will throw errors), and install the AbortSignal cross-realm bridge (jsdom's AbortController and Node's fetch are not in the same realm, so passing signals directly will throw exceptions).

After each test, automatically clean up the DOM, reset handlers, and reset fixture data/database to its initial state to prevent state leakage between tests. After all tests are finished, close MSW.

Also polyfilled matchMedia and ResizeObserver for AntD and ECharts to run in jsdom.
