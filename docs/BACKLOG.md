# Morpho Backlog

All work goes through this file. See CLAUDE.md for the workflow.

---

## Inbox

(empty — 2026-08-27 owner approved implementing the entire inbox, then building the APK and installing to phone via adb. All items triaged into In Progress below.)

## In Progress — wave-2 (dispatched 2026-08-27)

Phase ordering: engine code work and app bugfix are independent of the running
engine; content operations need the rebuilt Docker image (distractor v2 /
scorer v4 must be live); the ship chain runs last so the export captures every
content change exactly once.

| # | Title | Est | Pri | Phase | Status |
|---|-------|-----|-----|-------|--------|
| 19+8 | CLIP 语义评分内化到引擎 + codex 生成源（图文不匹配根因修复；worktree 内开发，不动线上） | L | P1 | 1 | dispatched |
| B3 | 学完单词后首页 0/50 不更新 — 代码级定位+修复+单测；真机复验放在收尾阶段 | S | P2 | 1 | dispatched |
| 9+B5 | NSFW/不当图片清理（CLIP 语义筛查全库 + 嫌疑词人工目检）+ 129 张 codex 图（126 在选）用 blank/floor gate 复检 reject | M | P1 | 2 | dispatched |
| 6 | 自指释义改写（粗查 126 个动词形匹配，权威集以 score_detail 为准）：in-scope 词表约束、防 OOV 回潮、mint→select→approve | M | P1 | 2 | dispatched |
| 12 | 存量干扰项 stem 坏配对修正（SQL 上限 2166 对）：无既有变更路径，需新增 core 批量 rebind 端点（dry-run 先行、优先 core_ready 替换以保出口稳定） | M→L | P1 | 2 | dispatched (worktree) |
| 17+20+21 | 发布链：合并 phase-1/2 代码 → Docker 重建部署 → unapprove/rescore/re-approve → **停容器后宿主原生跑 morphod publish**（容器内无 app/ 无 gradle，publish 不可能在容器里跑）→ APK → adb 卸载重装 | L | P1 | 3 | waiting |

关键发现（通读 README/contracts/publish.rs/compose 后）：
- publish 管线第 2-5 步触碰 app/ 与 gradle，而镜像 .dockerignore 排除了 app/ —— #17 必须以宿主原生 morphod 独占 DB 运行（先停容器，SQLite 单写者纪律）。
- publish 只自动补 content_version，不补 ReleaseDatabaseTest 的行数断言（§6.3 的 pin 设计）——行数变化时 gradle 步骤会红，需手动更新断言后重跑。
- conventions.md：subagent 不跑 git，版本控制归 conductor；docs/contracts 由 conductor 修改。

## Done — wave-2

| # | Title | Result |
|---|-------|--------|
| 5 | Docker 镜像重建+重启 | image `morpho-morphod:018c9ee9d6cc` @ 0ea2f5d；数据零漂移（6944 词/资产计数完全一致）；publish 子命令在位；无错误风暴。无代码变更 |

## Done (this session, 2026-08-27)

| # | Title | Commit |
|---|-------|--------|
| 1 | Home screen redesign (component architecture) | af9b9e4..2fec6f6 |
| 2 | Admin image gallery (infinite scroll) | 62a9ca0 |
| 3 | Docker deployment (port 30012) | 776e2f8 |
| 4 | Self-referencing definitions scorer fix (565→104) | 058e95f |
| 5 | Obscure primary senses / POS correction (482 words) | 058e95f, fa479fc |
| 7 | OOV cascade blocker (scorer v3) | 2ec573a |
| 10 | Mode-1 audio replay removed on correct | 0daedd9 |
| 11 | Mode 2/3 detail page behavior fixed | 0adaf8e |
| 13 | Per-word progress verified already correct | — |
| 14 | Image_file migrated to examples (code) | 74e37dc, 90227bd |
| 15 | Unified review mode | 2899f23 |
| 16 | Audio on wrong answer | 702a6f3 |
| 18 | Image scoring ignores source; no auto-pin | 611cb62 |
