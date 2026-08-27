# Morpho Backlog

All work goes through this file. See CLAUDE.md for the workflow.

---

## Inbox

Owner 方针（2026-08-27）：**先把 App 做到可用，bug 修复靠后**——当前可用性仍太低，wave-2 发布链优先。

| # | Title | Category | Description |
|---|-------|----------|-------------|
| 22 | 当日目标完成后可无限续学 | app | ⚠️ agent 提出，owner 未审核。`LearnViewModel.startSession()` 在 quota==0 时回填整批 dailyGoal 新词而非停止，与"今日完成"空态互相矛盾。待确认是 bug 还是有意的续学设计。B3 调查的顺手发现，暂不排期 |
| 23 | 无词可学时导航到旧总结页 | app | ⚠️ agent 提出，owner 未审核。学习计划为空时置 finished 但未生成会话结果，总结页显示上一次的数据。UX 毛边，暂不排期 |

## In Progress — wave-2 (dispatched 2026-08-27)

Phase ordering: engine code work and app bugfix are independent of the running
engine; content operations need the rebuilt Docker image (distractor v2 /
scorer v4 must be live); the ship chain runs last so the export captures every
content change exactly once.

| # | Title | Est | Pri | Phase | Status |
|---|-------|-----|-----|-------|--------|
| 19+8 | CLIP 语义评分内化到引擎 + codex 生成源（图文不匹配根因修复；worktree 内开发，不动线上） | L | P1 | 1 | dispatched |
| B3 | 首页 0/50 不更新 — 结论：HEAD 上不复现。历史真因是 wave-3a 的 daily_stats schema 变更未写迁移，旧 user.db 上所有统计写入失败；2a8e2ed 的预发布库丢弃守卫已修。已补 5 个钉死测试（03e7bf5，42 test 全绿）。真机复验留在装机后 | S | P2 | 1 | ✅ code-verified |
| 9+B5 | NSFW/不当图片清理（CLIP 语义筛查全库 + 嫌疑词人工目检）+ 129 张 codex 图（126 在选）用 blank/floor gate 复检 reject。**Owner 追加（2026-08-27 已确认）**：全库在选图清除 ①纯色/近纯色图 ②以文字为主体的图（尤其把目标词写在图里的——泄答案且骗 CLIP 分）；文字类每张人工目检后才 reject | M | P1 | 2 | dispatched, scope extended |
| 6 | 自指释义改写 | M | P1 | 2 | ✅ done, conductor-verified |
| 12 | 存量干扰项 stem 坏配对修正 — 代码完成并合并（d80c194）：`POST /distractors/rebind-violations`，共享 ranked_candidates 选择逻辑，core_ready 池，乐观守卫，698 测试绿。**待办**：phase-3 部署新引擎后、导出前，dry-run → apply → 复扫为零；admin-ui types.ts 镜像另记 | M→L | P1 | 2 | code merged, live run pending deploy |
| 17+20+21 | 发布链：合并 phase-1/2 代码 → Docker 重建部署 → unapprove/rescore/re-approve → **停容器后宿主原生跑 morphod publish**（容器内无 app/ 无 gradle，publish 不可能在容器里跑）→ APK → adb 卸载重装 | L | P1 | 3 | waiting |

关键发现（通读 README/contracts/publish.rs/compose 后）：
- publish 管线第 2-5 步触碰 app/ 与 gradle，而镜像 .dockerignore 排除了 app/ —— #17 必须以宿主原生 morphod 独占 DB 运行（先停容器，SQLite 单写者纪律）。
- publish 只自动补 content_version，不补 ReleaseDatabaseTest 的行数断言（§6.3 的 pin 设计）——行数变化时 gradle 步骤会红，需手动更新断言后重跑。
- conventions.md：subagent 不跑 git，版本控制归 conductor；docs/contracts 由 conductor 修改。

## Done — wave-2

| # | Title | Result |
|---|-------|--------|
| 5 | Docker 镜像重建+重启 | image `morpho-morphod:018c9ee9d6cc` @ 0ea2f5d；数据零漂移（6944 词/资产计数完全一致）；publish 子命令在位；无错误风暴。无代码变更 |
| ops | morpho-genimg.timer 关闭 | Owner 要求。disable --now 并验证（disabled/inactive/列表清零）。wave2 列表 221 词已生成 81、剩 140 不再走此野路子——未来由 #19 引擎内建生成源接管。（更正：先前记录"最后一批 81 张 0 胜出"系误读——那次 firing 是对已上传图的重复 ingest，incumbent==generated 自比自，kept 是 no-op。真实战绩：wave-1 48/48 胜出、wave-2 全部有非 manual 对手的 77 张胜出）|
| 6 | 自指释义清零 | 权威集 271 处（266 词，远超估计的 104；粗查 126 是因 LIKE 漏屈折形和标点边界）。264 条新撰 + 7 条改选既有干净候选，全部 manual 带血缘。conductor 独立复核：自指 0、§7.10 未解析词元 0、oos_queue 与基线一致、TTS 收敛 missing/failed/死信全 0、blocked 维持 7。两个 agent 判断已采纳：超 200 停止线继续（机械改写+机械校验成立）；9 个退化义项（owl=鸽子、source=源代码等）顺手改为常用义。副作用：33 个辅助词因新释义用词更平实而失去引用、自动退休（1146→1113，设计内可逆）。日志 ops/logs/defrewrite-2026-08-27.json |

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
