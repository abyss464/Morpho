# Morpho Backlog

Canonical source of work items. Two entry points: `report` (add) and `triage` (evaluate).

## Workflow

```
User reports problem/idea
        |
   [ Inbox ]  ← recorded immediately, no code touched
        |
   Triage pass ← assess effort, priority, dependencies, feasibility
        |
   ┌────┴────┐
[ Ready ]  [ Deferred / Won't-do + reason ]
   |
[ In Progress ]  ← one item at a time unless parallelizable
   |
[ Done ]  ← committed, tested, verified
```

**Effort**: S (< 30 min), M (1-3 h), L (half day), XL (full day+)
**Priority**: P0 (blocks usage), P1 (degrades experience), P2 (improvement), P3 (nice-to-have)
**Category**: `app` `engine` `admin` `ops` `content` `infra`

---

## Inbox

_New items land here. Not evaluated yet._

<!-- template:
| # | Title | Category | Reporter note |
|---|-------|----------|---------------|
-->

## Ready

_Triaged, estimated, approved for implementation. Ordered by priority then effort._

| # | Title | Cat | Pri | Effort | Notes |
|---|-------|-----|-----|--------|-------|

## In Progress

| # | Title | Started | Branch/Commit |
|---|-------|---------|---------------|

## Done

| # | Title | Closed | Commit |
|---|-------|--------|--------|
| 1 | Home screen redesign (component architecture) | 2026-08-27 | af9b9e4..2fec6f6 |
| 2 | Admin image gallery (infinite scroll) | 2026-08-27 | 62a9ca0 |
| 3 | Docker deployment (port 30012) | 2026-08-27 | 776e2f8 |
| 4 | Self-referencing definitions | 2026-08-27 | 058e95f |
| 5 | Obscure primary senses / POS correction | 2026-08-27 | 058e95f, fa479fc |
| 6 | Image-sentence mismatch (CLIP + codex gen) | 2026-08-27 | 925bed8, f03b64f |
| 7 | OOV cascade blocker (scorer v3) | 2026-08-27 | 2ec573a |
| 8 | Release 1.5 | 2026-08-27 | d4f308c |

## Deferred

| # | Title | Reason |
|---|-------|--------|
| D1 | NSFW pipeline filter (CLIP classifier) | Gallery manual screening sufficient for now; revisit when image sources expand |
| D2 | Distractor semantic near-duplication | Design needed; contain/container both yield container scenes |
| D3 | ADB overlay install corruption | Workaround: uninstall first; root cause investigation deferred |

## Won't Do

| # | Title | Reason |
|---|-------|--------|
