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
| # | Title | Category | Reporter note |
|---|-------|----------|---------------|
| 18 | 图片评分去掉 source_prior + 自动上传不 pin | engine/ops | source 只是标签不参与评分。CLIP 置信度决定选择。只有人在 admin UI 手动点"Use this"才 pin。脚本/引擎自动上传的候选不 pin，参与正常置信度竞争 |
| 19 | codex 图片生成适配器 + admin 手动触发 | engine/admin | 和 SDXL 并行的可选生成源。引擎内置速率限制 (5h/80张)。admin Gallery 可手动选低质量图触发生成。source 标记为 codex，不 pin，按置信度竞争 |
| 17 | 发布链路自动化 morphod publish | core/admin | morphod 新子命令：export → cp release.db → rsync media → 更新测试断言 → gradle build。admin API 留端点 |

## Ready

_Triaged, estimated, approved for implementation. Ordered by priority then effort._

| # | Title | Cat | Pri | Effort | Notes |
|---|-------|-----|-----|--------|-------|
|
|
|
|
|
| 14 | 图片绑到 example slot 1 + CLIP rematch | engine/app/ops | P2 | L | schema: release.db 图字段从 words.image_file 移到 examples.image_file (display_order=1)。engine export 适配。App ContentRepository 从 examples 读图。现有图平移。全词库 CLIP rematch 用纯例句查询。working.db image_selections 主键不变（第一阶段仍每词一图） |
| 17 | 发布链路自动化 morphod publish | core/admin | P2 | L | morphod 新子命令 publish：export → cp release.db → rsync media → 更新测试断言 → gradle build，一条命令完成。admin API 留 POST /api/releases/publish 端点。不含 adb install |

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
| 15 | 统一复习模式：单轮模式 2（4 图+释义） | 2026-08-27 | 2899f23 |
| 12 | 干扰项排除形变词 + 同 POS 优先 | 2026-08-27 | ccbc386 |

## Deferred

| # | Title | Reason |
|---|-------|--------|
| D1 | NSFW pipeline filter (CLIP classifier) | Gallery manual screening sufficient for now; revisit when image sources expand |
| D2 | Distractor semantic near-duplication | Design needed; contain/container both yield container scenes |
| D3 | ADB overlay install corruption | Workaround: uninstall first; root cause investigation deferred |

## Won't Do

| # | Title | Reason |
|---|-------|--------|
