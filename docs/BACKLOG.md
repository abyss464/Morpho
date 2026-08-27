# Morpho Backlog

All work goes through this file. See CLAUDE.md for the workflow.

---

## Inbox

| # | Title | Category | Description |
|---|-------|----------|-------------|
| 6 | 104 个自指释义未修复 | content | scorer v3 把 565 降到 104，但剩余 104 个无干净候选，需要 LLM 改写或人工录入 |
| 8 | 图文不匹配未真正修复 | engine | CLIP rematch 两次都 changed=0（pin 机制阻挡+引擎不内置 CLIP）。实际图片选择没有按语义优化 |
| 9 | NSFW/不当图片未清理 | content | 发现了 naked/breast/desire/flesh/thigh/rape 等问题图，但只做了发现没做 reject 清理 |
| 12 | 现有干扰项中的形变词未修正 | content | 新算法只影响未来绑定（494 词），已绑定的 4508 词里的坏配对（adapt/adapter 等）没有修正 |
| 19 | CLIP 语义评分内化到引擎 + codex 适配器 | engine | 根因修复：引擎 auto_score 不看语义。需要 CLIP 适配器子进程 + codex 图片生成源 |
| 20 | App 图标和加载动画未进入 APK | app | 代码已提交 (7f64328) 但没有重新导出+打包，手机上还是旧图标 |
| 21 | 当前 APK 需要重新导出验证 | ops | 图片经历了全量 unpin→re-approve，加上 #14 #18 #20 代码变更都没进 APK |
| B3 | 学完单词后首页 0/50 不更新 | app | 用户报告的 bug，未在实机确认是否修复 |
| B5 | codex 生成图质量问题残留 | content | verify_genimg 加了 blank/floor gate，但没有用新 gate 重新过滤已上传的 129 张 codex 图 |
| 5 | Docker 镜像未包含最新代码 | infra | scorer v4、#14 schema 变更、#17 publish 命令、#20 图标等都提交了但 Docker 镜像没重建 |
| 17 | morphod publish 命令未验证 | core | 代码已提交 (72598b8) 但从未实际运行过 |

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
