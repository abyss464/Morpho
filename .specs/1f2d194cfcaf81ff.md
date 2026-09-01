---
file: core/crates/api/src/dto.rs
---

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
