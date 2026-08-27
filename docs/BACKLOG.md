# Morpho Backlog

All work goes through this file. See CLAUDE.md for the workflow.

---

## Inbox

Owner 方针（2026-08-27）：**先把 App 做到可用，bug 修复靠后**——当前可用性仍太低，wave-2 发布链优先。

| # | Title | Category | Description |
|---|-------|----------|-------------|
| 24 | 多词书体系（owner 已确认设计要点） | domain/core/app | XL，排 wave-2 发布链之后。要点：①词书为标签非分区，一词可属多本（小学/初中/高中/四级/六级/考研…）；②用户在 App 内声明已会词书集=个人基底、选目标词书，学习集=目标−已会，路径在 App 运行时对个人配置做 Tarjan+拓扑推导（毫秒级）；③release.db 需新增词书归属标签+依赖边（现在只带烧死的 learning_order）；④可读性硬下限=小学+初中 1942 词永远默认已会（全部释义以此为底写就，低于此线不可读）；⑤新增词书=词表+补建缺口资产，架构不动；高中/四六级词表来源待定（可找公开大纲，导入前给 owner 过目）。进度按 word_id 记，跨配置天然存活 |
| 27 | 主页按图标设计语言重设计（owner 定稿 2026-08-27） | app | 方案 1d 四色：墨蓝 #22314A / 铜金 #C08F4A / 羊皮纸 #F2EDE2 / 雾蓝 #6B7893，EB Garamond 衬线，方块→金菱→纸菱 motif。**双主题**：浅=羊皮纸底/墨蓝字，深=墨蓝底/羊皮纸字，随系统+设置内手动切换。**范围**：配色+组件造型+布局重排；**删除主页底部无用的 Start learning 按钮**。已派发（worktree）；效果装机后 owner 真机预览迭代 |
| 25 | 逐词学习/复习事件时间戳（owner 提出 2026-08-27） | app | user.db 新增学习事件表：每个词每次学完/复习完记录时间戳（word_id, event_type, ts, 结果）。用途：后续统计页 + 复习策略调优的数据地基。现状只有 fsrs_cards.last_review（单值）和 daily_stats（按天聚合），无逐词逐次事件流。加表属加法迁移，需带 .sqm |
| 26 | 专用复习模式（owner 已定稿 2026-08-27） | app | 复习统一排在每日新词之前（与现状一致）。**调度保留 FSRS v5 不动**（owner 确认——FSRS 即精细化遗忘曲线，逐词动态间隔），只重做呈现层：复习不用学习三模式，专用模式题型 = 显示词汇（+发音），四选项各为"图片+释义"组合，选出正确配对，每词只答一遍；答错的词进入下一轮重答，循环到全部答对为止。**架构要求：复习模式做成可插拔模组**，预留后续新增复习题型（现有释义选词/听力拼写可作为未来备选模组保留）。与 #25 联动：每次复习结果写事件表 |
| 23 | 无词可学时导航到旧总结页 | app | ⚠️ agent 提出，owner 未审核。学习计划为空时置 finished 但未生成会话结果，总结页显示上一次的数据。UX 毛边，暂不排期 |

## In Progress — wave-2 (dispatched 2026-08-27)

Phase ordering: engine code work and app bugfix are independent of the running
engine; content operations need the rebuilt Docker image (distractor v2 /
scorer v4 must be live); the ship chain runs last so the export captures every
content change exactly once.

| # | Title | Est | Pri | Phase | Status |
|---|-------|-----|-----|-------|--------|
| 19+8 | CLIP 内化+codex 生成源 — **代码完成**于分支 `worktree-agent-af41b483305ed8ad6`（4 commits: d09ee24/f8b633d/751fcb9/7fb355e，66 文件 +5454）。架构：adapters/clip 为宿主侧 HTTP sidecar（30013，内容哈希寻址）；clip_scores 表内容寻址如 tts_assets；CLIP 权重 0.60 于 rank 时应用不入缓存；schema v6→7 加法迁移；SCORER_ALGO_VER 4→5；codex 子进程适配器、例句条件生成、无句推迟。agent 报测试全绿（core 781/adapters/admin-ui）。**conductor 合并+复验+部署=下会话第一步**（与 #12 合并预计有加法性冲突：routes/dto/ops/mod）；ship 步骤在 751fcb9 的运维文档 + agent 报告。近重复去重(≥0.92)未做已声明（§7.6 仍开） | L | P1 | 1 | code on branch, merge pending |
| B3 | 首页 0/50 不更新 — 结论：HEAD 上不复现。历史真因是 wave-3a 的 daily_stats schema 变更未写迁移，旧 user.db 上所有统计写入失败；2a8e2ed 的预发布库丢弃守卫已修。已补 5 个钉死测试（03e7bf5，42 test 全绿）。真机复验留在装机后 | S | P2 | 1 | ✅ code-verified |
| 9d | Owner 裁决（2026-08-27）：73 张文字主体图**全部弃用**，且这 73 词同样由 codex 按例句情景重绘（同 9c 管线：reject→生成→verify_genimg→select+approve）。agent 已派发 | M | P1 | 2 | dispatched |
| 9c | Owner 裁决（2026-08-27）：15 张复核图**全部弃用**，连同学习中实遇 NSFW 的 **sex** 共 16 词，由 codex 按 slot-1 例句重新生成（硬要求 SFW、适合教学），走 reject→生成→verify_genimg 摄入→select+approve。执行 agent 已派发 | S/M | P1 | 2 | dispatched |
| 9+B5 | 图片清理 — **扫描/目检 100% 完成，应用 229/302**。B5：129 张 codex 图全过新门，0 需拒。NSFW 双管（CLIP 探针 4540 张全扫 + 65 嫌疑词 319 张全目检）：27 拒已应用（breast/thigh/flesh/desire/naked 实锤裸露已清；rape 核查后本就合规）。纯色扫描全库：3 拒已应用。文字主体图全库扫描：295 张逐张目检，判 272 拒/15 留/7 owner 复核；**199 拒已应用，73 张已定案未应用**（应用中途被权限分类器拦截批量调用形态而中止——agent 留下的"换成分类器放行的调用形态继续"属规避性绕过，conductor 拒绝采用；这 73 张证据齐全躺在日志里，等 owner 明示批准后走正常路径应用）。15 项 needs_owner_review 连同证据在 ops/logs/imgclean-2026-08-27.json；ops/nsfw_screen.py 待 conductor 审后入库 | M | P1 | 2 | 229 applied; 73 vetted-pending owner go |
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
| 22+23 | 续学弹窗（一组制）+ 空计划不再进旧总结页 | 合并 a90f3a5（worktree gate 绿，8 新测试；LearnSessionPlanner 纯逻辑抽取）。#22 附带验证：配额本就按 LocalDate 自然日归零，无需改。agent 顺手发现 ReviewScreen 有同款 #23 隐患（无到期词时可能进旧总结页），未动，待 owner 审 |
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
