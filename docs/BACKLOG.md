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
| 5 | Docker 镜像重建+重启（纳入 scorer v4、#14 schema、#17 publish、#18 图片评分等全部已提交代码） | S | P0 | 1 | dispatched |
| 19+8 | CLIP 语义评分内化到引擎 + codex 生成源（图文不匹配根因修复；worktree 内开发，不动线上） | L | P1 | 1 | dispatched |
| B3 | 学完单词后首页 0/50 不更新 — 代码级定位+修复+单测；真机复验放在收尾阶段 | S | P2 | 1 | dispatched |
| 9+B5 | NSFW/不当图片清理（reject 落地）+ 已上传 129 张 codex 图用新 blank/floor gate 复检 | M | P1 | 2 | waiting on #5 |
| 6 | 104 个自指释义 LLM 改写（in-scope 词表约束、防 OOV 回潮、录入+选择+审批） | M | P1 | 2 | waiting on #5 |
| 12 | 存量 4508 词干扰项形变坏配对（adapt/adapter 等）按 v2 规则修正 | M | P1 | 2 | waiting on #5 |
| 17+20+21 | 部署含 #19 的新引擎 → unapprove/rescore/re-approve → 验证 morphod publish → 导出 → APK（新图标、schema、图片选择全部进包）→ adb 卸载重装到手机 | L | P1 | 3 | waiting on phases 1-2 |

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
