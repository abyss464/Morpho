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

_(empty)_

## Ready

_Triaged, estimated, approved for implementation. Ordered by priority then effort._

| # | Title | Cat | Pri | Effort | Notes |
|---|-------|-----|-----|--------|-------|
| 19 | CLIP 语义评分内化到引擎 + codex 适配器 | engine | P1 | XL | 当前 auto_score 只看元数据（分辨率+POS），不看图文语义匹配。应该：引擎在图片候选入库时自动调 CLIP 适配器（子进程），语义分数写入 auto_score，自动选择天然选最匹配的图。codex 图片生成作为并行图片源适配器。外部 clip_rematch.py / verify_genimg.py 退役。需设计：Rust 调 Python CLIP 的适配器协议 |

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
| 9 | CLIP score floor + blank-image gate in verify_genimg | 2026-08-27 | be61804 |
| 10 | Mode-1 orphaned sentence replay removed | 2026-08-27 | 0daedd9 |
| 11 | Mode 2/3 skip detail on correct; full-expand on wrong | 2026-08-27 | 0adaf8e |
| 12 | Distractor stem exclusion + same-POS preference | 2026-08-27 | ccbc386 |
| 13 | Per-word progress verified correct | 2026-08-27 | already implemented |
| 14 | Image_file migrated to examples table | 2026-08-27 | 74e37dc, 90227bd |
| 15 | Unified review mode (single mode-2 visual) | 2026-08-27 | 2899f23 |
| 16 | Audio on wrong answer | 2026-08-27 | 702a6f3 |
| 17 | morphod publish automation | 2026-08-27 | 72598b8 |
| 18 | Image scoring ignores source; no auto-pin | 2026-08-27 | 611cb62 |
| 20 | App icon + flow loading animation | 2026-08-27 | 7f64328 |

## Deferred

| # | Title | Reason |
|---|-------|--------|
| D1 | NSFW pipeline filter (CLIP classifier) | Gallery manual screening sufficient; revisit when #19 lands (CLIP in engine) |
| D2 | Distractor semantic near-duplication | Partially solved by #12 stem exclusion; residual (visual overlap for unrelated words) low impact |
| D3 | ADB overlay install corruption | Workaround: `pm clear` + reinstall; root cause deferred |

## Won't Do

| # | Title | Reason |
|---|-------|--------|
