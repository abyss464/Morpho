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

| # | Title | Category | Reporter note |
|---|-------|----------|---------------|
_(all moved to Ready or pending clarification)_

## Ready

_Triaged, estimated, approved for implementation. Ordered by priority then effort._

| # | Title | Cat | Pri | Effort | Notes |
|---|-------|-----|-----|--------|-------|
|
|
| 15 | 复习模式改造：每日先复习 + 单轮模式 2 | app | P1 | L | 复习不走三轮模式，改为单一轮次：上方单词，下方 4 图+释义（模式 2 样式）。每天先完成到期复习词，再开始新词学习。复习调度基于已有 FSRS v5（已实现到期计算），但题型统一为模式 2 而非当前的释义选词+听写双模式 |
|
| 12 | 干扰项排除形变词 | engine | P2 | L | 当前纯编辑距离，不排除同词族（adapt/adapter 会配对）。需要词族/词根数据或 stem 比较。且干扰项永久绑定——已绑定的不会自动更新 |
| 14 | 图片绑定迁移到 example (slot 1) | engine/app | P2 | L | 架构级。image_selections 主键从 word_id 改为 (word_id, slot)，release.db 图字段从 words 移到 examples。现有图平移到 slot 1 |

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
| 10 | Mode-1 orphaned sentence replay removed (半完成，见 #16) | 2026-08-27 | 0daedd9 |
| 11 | Mode 2/3 skip detail on correct; full-expand on wrong | 2026-08-27 | 0adaf8e |
| 13 | 逐词进度已确认正确（三轮通关=已学+FSRS时间戳） | 2026-08-27 | 已实现，无需改动 |
| 16 | 答错后自动播放一次音频（模式1=句子，2/3=单词） | 2026-08-27 | 702a6f3 |

## Deferred

| # | Title | Reason |
|---|-------|--------|
| D1 | NSFW pipeline filter (CLIP classifier) | Gallery manual screening sufficient for now; revisit when image sources expand |
| D2 | Distractor semantic near-duplication | Design needed; contain/container both yield container scenes |
| D3 | ADB overlay install corruption | Workaround: uninstall first; root cause investigation deferred |

## Won't Do

| # | Title | Reason |
|---|-------|--------|
