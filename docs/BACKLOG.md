# Morpho Backlog

All work goes through this file. See CLAUDE.md for the workflow.

---

## Inbox

Owner 方针（2026-08-27）：**先把 App 做到可用，bug 修复靠后**——当前可用性仍太低，wave-2 发布链优先。

| # | Title | Category | Description |
|---|-------|----------|-------------|
| 38 | 崩溃诊断通道 | app | 目标：外部用户设备上的崩溃可被开发侧定位。交付形态待 owner 选型——候选 A：本地捕获+用户手动导出分享（维持全离线架构）；候选 B：远程自动上报（需网络权限与收集端）。验收：任一外部设备崩溃后，开发侧可获得含完整堆栈、机型、系统版本、应用与内容版本的日志 |
| 39 | vivo S30 Pro mini「开始学习」崩溃 | app | 复现环境：vivo S30 Pro mini，1.7 fatApkDebug。路径：经 #38 日志通道取证 → 定位 → 修复。验收：该机型进入并完成学习流程稳定可用 |
| 24 | 多词书体系（owner 已确认设计要点） | domain/core/app | XL，排 wave-2 发布链之后。要点：①词书为标签非分区，一词可属多本（小学/初中/高中/四级/六级/考研…）；②用户在 App 内声明已会词书集=个人基底、选目标词书，学习集=目标−已会，路径在 App 运行时对个人配置做 Tarjan+拓扑推导（毫秒级）；③release.db 需新增词书归属标签+依赖边（现在只带硬编码的 learning_order）；④可读性硬下限=小学+初中 1942 词永远默认已会（全部释义以此为底写就，低于此线不可读）；⑤新增词书=词表+补建缺口资产，架构不动；高中/四六级词表来源待定（可找公开大纲，导入前给 owner 过目）。进度按 word_id 记，跨配置天然存活 |
| 33 | 全库主释义连贯性审计（owner 确认记录 2026-08-27，暂不派发） | content/engine | 现状：自指门压低常用义后，生僻但 OOV-干净的义项可登顶主义项，且动词槽可整体缺失（样本 roll：主义项为斗狗生僻义）。目标：全库主释义与例句语义一致、词性槽完整。手段三路：①LLM 评审全库 (词,主释义,例句) 三元组，抓义句错配与生僻义；②SQL 抓例句词性无 enabled 槽、freedict 单词性词；③WordNet 义项排位预筛。修复复用 #6 管线（改选/新撰 + in-scope 校验 + 血缘 + §7.10 复查）。引擎侧根治：义项常用度权重需压过"生僻但干净"。规模≈#6 的 3 倍 |
| 34 | 学习内容文本不用衬线（owner 定稿 2026-08-27） | app | EB Garamond 太细，学习中的正文（例句/释义等内容文本）改回无衬线；衬线只保留在 UI 装饰层（标题、数字、品牌元素）。#27 的 Type.kt reading styles 需回调 |
| 37 | 【owner 要求】必须支持覆盖安装升级 | app/ops | 现状：overlay 安装会损坏 App，只能 uninstall→install，进度随之丢失，与 README"覆盖安装天然保 user.db"的设计相悖。目标：覆盖安装即完成升级并保留进度。工作项：①定位并修复 overlay 损坏（嫌疑：fatApk 巨型 assets 变更 + AssetManager/安装器交互、或 dexopt/签名缓存）；②修不掉则提供等效替代（安装前自动备份 user.db + 装后还原的官方通道）。验收：旧版在装、直接 `adb install -r` 新版 → App 可用 + 进度保留 + content_version 对账正确执行。过渡期装机走"拉备份→重装→灌回" | 
| 36 | 复习 reviewed 计数器同款批量结算 | app | ⚠️ agent 提出，owner 未审核。现状：ReviewViewModel.finish() 与 #35 同形状，会话终点才写 daily_stats.reviewed；复习队列短、卡片逐题落盘，影响有限。目标：与 #35 的 SessionBank 对齐。暂不排期 |
| 32 | 画廊按匹配度逆序筛选不合格图（owner 提出 2026-08-27） | admin | **已派发（worktree）**。画廊 sort=clip_asc（最差在前）+ 卡片显示 CLIP 分 + selected_only 筛选；core /gallery 扩展 + admin-ui 控件。另注：codex 上传图现挂 manual 标签（上传端点固定 manual），源筛选语义需澄清或上传 API 加 source 参数 |
| 28 | 手动深色主题下冷启动闪白 | app | ⚠️ agent 提出，owner 未审核。现状：窗口背景跟随系统夜间模式而非应用内手动选择，浅色系统 + App 选 Dark 时每次启动先显示浅色再切深色。目标：启动首帧即为应用内选定主题。取舍：UiModeManager.setApplicationNightMode 为 API 31+ 且会影响 SYSTEM 模式判定，需设计方案。暂不排期 |
| 25 | 逐词学习/复习事件时间戳（owner 提出 2026-08-27） | app | user.db 新增学习事件表：每个词每次学完/复习完记录时间戳（word_id, event_type, ts, 结果）。用途：后续统计页 + 复习策略调优的数据地基。现状只有 fsrs_cards.last_review（单值）和 daily_stats（按天聚合），无逐词逐次事件流。加表属加法迁移，需带 .sqm |
| 26 | 专用复习模式（owner 已定稿 2026-08-27） | app | 复习统一排在每日新词之前（与现状一致）。**调度保留 FSRS v5 不动**（owner 确认——FSRS 即精细化遗忘曲线，逐词动态间隔），只重做呈现层：复习不用学习三模式，专用模式题型 = 显示词汇（+发音），四选项各为"图片+释义"组合，选出正确配对，每词只答一遍；答错的词进入下一轮重答，循环到全部答对为止。**架构要求：复习模式做成可插拔模组**，预留后续新增复习题型（现有释义选词/听力拼写可作为未来备选模组保留）。与 #25 联动：每次复习结果写事件表 |
| 23 | 无词可学时导航到旧总结页 | app | ⚠️ agent 提出，owner 未审核。现状：学习计划为空时置 finished 但未生成会话结果，总结页显示上一次的数据。目标：空计划不进入总结页。暂不排期 |

## In Progress — wave-2 (dispatched 2026-08-27)

Phase ordering: engine code work and app bugfix are independent of the running
engine; content operations need the rebuilt Docker image (distractor v2 /
scorer v4 must be live); the ship chain runs last so the export captures every
content change exactly once.

| # | Title | Est | Pri | Phase | Status |
|---|-------|-----|-----|-------|--------|
| 19+8 | CLIP 内化+codex 生成源 — 代码在分支 `worktree-agent-af41b483305ed8ad6`（4 commits: d09ee24/f8b633d/751fcb9/7fb355e，66 文件 +5454）。架构：adapters/clip 为宿主侧 HTTP sidecar（30013，内容哈希寻址）；clip_scores 表内容寻址如 tts_assets；CLIP 权重 0.60 于 rank 时应用不入缓存；schema v6→7 加法迁移；SCORER_ALGO_VER 4→5；codex 子进程适配器、例句条件生成、无句推迟。agent 自报测试全绿（core 781/adapters/admin-ui），待独立复验。下一步：合并 + 复验 + 部署（与 #12 合并预计有加法性冲突：routes/dto/ops/mod）；ship 步骤见 751fcb9 的运维文档。近重复去重(≥0.92)未实现，§7.6 仍开 | L | P1 | 1 | code on branch, merge pending |
| B3 | 首页 0/50 不更新 — HEAD 上不复现。成因：wave-3a 的 daily_stats schema 变更未写迁移，旧 user.db 上统计写入全部失败；00df8fc 的预发布库丢弃守卫已覆盖。补 5 个回归测试（fedb00b，42 test 绿）。真机复验待装机 | S | P2 | 1 | ✅ code-verified |
| 9d | Owner 裁决（2026-08-27）：73 张文字主体图全部弃用，这 73 词由 codex 按例句情景重绘（同 9c 管线：reject→生成→verify_genimg→select+approve） | M | P1 | 2 | dispatched |
| 9c | Owner 裁决（2026-08-27）：15 张复核图全部弃用，连同学习中遇到 NSFW 的 **sex** 共 16 词，由 codex 按 slot-1 例句重新生成（硬要求 SFW、适合教学），走 reject→生成→verify_genimg 摄入→select+approve | S/M | P1 | 2 | dispatched |
| 9+B5 | 图片清理 — 扫描/目检 100% 完成，应用 229/302。B5：129 张 codex 图全过新门，0 需拒。NSFW 双管（CLIP 探针 4540 张全扫 + 65 嫌疑词 319 张全目检）：27 拒已应用（breast/thigh/flesh/desire/naked 裸露已清；rape 核查后合规）。纯色扫描全库：3 拒已应用。文字主体图全库扫描：295 张逐张目检，判 272 拒/15 留/7 owner 复核；199 拒已应用，73 张已定案未应用——应用批量调用被权限系统拦截而中止，证据齐全在日志中，待 owner 批准后走正常路径应用。15 项 needs_owner_review 连同证据在 ops/logs/imgclean-2026-08-27.json；ops/nsfw_screen.py 待审后入库 | M | P1 | 2 | 229 applied; 73 vetted-pending owner go |
| 6 | 自指释义改写 | M | P1 | 2 | ✅ done, conductor-verified |
| 12 | 存量干扰项 stem 坏配对修正 — 代码已合并（49d0f30）：`POST /distractors/rebind-violations`，共享 ranked_candidates 选择逻辑，core_ready 池，乐观守卫，698 测试绿。待办：phase-3 部署新引擎后、导出前，dry-run → apply → 复扫为零；admin-ui types.ts 镜像另记 | M→L | P1 | 2 | code merged, live run pending deploy |
| 29 | morphod publish 的两个 bug | core | ①repo_root 解析：resolve_adapters_root 存仓库根而 repo_root() 取其 parent，语义冲突致 app/ 路径错一级（绕过：显式 MORPHOD_ADAPTERS_ROOT）；②stale 清理只扫 img/、audio/ 子目录，扫不到旧平铺布局的根级遗留（25904 个文件 492MB，人工 gio trash 清除后 APK 983→532MB）。另：publish 不补测试体内的计数断言（plan size/checked/gloss index），§6.3 需记录 |
| 30 | CLIP 重选后不当图回流（重选后需复扫） | content/engine | 现状：文字图清扫跑在 CLIP 重选之前，池中未在选的问题图被重选抬进槽位。两例：author 的"AUTHOR & SPEAKER"文字卡（owner 裁决：可留）；section 选中剖腹产疤痕特写（wikimedia 产科手术图 932/930/931/728 已全数 reject，改选牛油果剖面 933 并批准）。目标：重选后新在选集重跑 NSFW+text 双筛 + 人审；引擎端对该类图降权。CLIP 分位统计（2026-08-27）：4127 在选已评分中 <0.08 仅 8 词、0.08-0.15 弱匹配 725 词 |
| 31 | reject 后 human 选择行卡死不回退 | core | 现状（样本 section/4983）：reject 在选候选后，选择行为 selected_by=human, pinned=0 且仍指向 rejected 候选，自动重选规则只处理 auto 行，槽位永久悬挂，只能人工改选。与 §7.5 的 pin-fallback 设计不符 |

部署约束：
- publish 管线第 2-5 步触碰 app/ 与 gradle，而镜像 .dockerignore 排除了 app/ —— #17 必须以宿主原生 morphod 独占 DB 运行（先停容器，SQLite 单写者纪律）。
- publish 只自动补 content_version，不补 ReleaseDatabaseTest 的行数断言（§6.3 的 pin 设计）——行数变化时 gradle 步骤会红，需手动更新断言后重跑。
- conventions.md：subagent 不跑 git，版本控制归 conductor；docs/contracts 由 conductor 修改。

## Done — wave-2

| # | Title | Result |
|---|-------|--------|
| 5 | Docker 镜像重建+重启 | image `morpho-morphod:018c9ee9d6cc` @ e82c7fa；数据零漂移（6944 词/资产计数一致）；publish 子命令在位。无代码变更 |
| ops | morpho-genimg.timer 关闭 | Owner 要求。disable --now 并验证（disabled/inactive/列表清零）。wave2 列表 221 词已生成 81，剩 140 由 #19 引擎内建生成源接管。累计战绩：wave-1 48/48 胜出，wave-2 有非 manual 对手的 77 张全部胜出（先前记录的"最后一批 81 张 0 胜出"为对已上传图重复 ingest 造成的自比自，kept 是 no-op）|
| 35 | 进度 0/50 根治+实机验证 | 根因：daily_stats 只在整个 50 词计划全部走完时结算一次（wave-1 设计，非回归），且 0 计数自续命。修复 7d246d6：SessionBank 水位线逐题入账、幂等、跨午夜分账，5 新测试。1.7.x 补丁装机（028ed74 含 #34），user.db 备份→回灌 + 今日 22 词补记，实机截图确认 22/50 |
| 34 | 学习内容回无衬线 | 5cb0bb8→028ed74 合并：readingFontFamily 单点根修级联 6 内容样式，chrome 保衬线，一并修正 RetryHelpCard 字体不一致 |
| 17+20+21 | 发布链走通，release 1.7 装机 | `2026.08.27+0847bb29`，4225 词，APK 532MB（陈旧媒体清理后减半），干净卸载重装（真实包名 dev.morpho.debug，首装==更新时间戳已验证），实机启动 + 截图确认新主题生效。#17 publish 首验发现 2 bug → #29。commit cbedd76 |
| 27 | 主页重设计（图标语言/双主题/motif/删冗余入口） | 合并 fa9ebfb（76c7524，22 文件 +1032/−484）：四色 ramp、EB Garamond 内嵌（OFL 留档）、Motif.kt 组件族、Settings 内 System/Light/Dark 切换、QuickAccessRow 删除。gate 绿。待办：docs/contracts/app-design.md Brand 段落仍写旧 morpho blue，发布后更新 |
| 22+23 | 续学弹窗（一组制）+ 空计划不再进旧总结页 | 合并 6917c92（worktree gate 绿，8 新测试；LearnSessionPlanner 纯逻辑抽取）。#22 附带验证：配额本就按 LocalDate 自然日归零，无需改。ReviewScreen 有同款 #23 隐患（无到期词时可能进旧总结页），未动，待 owner 审 |
| 6 | 自指释义清零 | 权威集 271 处（266 词；粗查 126 偏低是 LIKE 漏屈折形和标点边界所致）。264 条新撰 + 7 条改选既有干净候选，全部 manual 带血缘。独立复核：自指 0、§7.10 未解析词元 0、oos_queue 与基线一致、TTS 收敛 missing/failed/死信全 0、blocked 维持 7。附带处理：9 个退化义项（owl=鸽子、source=源代码等）改为常用义。副作用：33 个辅助词因新释义用词更平实而失去引用、自动退休（1146→1113，设计内可逆）。日志 ops/logs/defrewrite-2026-08-27.json |

## Done — wave-1 (2026-08-27)

| # | Title | Commit |
|---|-------|--------|
| 1 | Home screen redesign (component architecture) | b9a0887..e472155 |
| 2 | Admin image gallery (infinite scroll) | 109398e |
| 3 | Docker deployment (port 30012) | a9a81ec |
| 4 | Self-referencing definitions scorer fix (565→104) | b84bd89 |
| 5 | Obscure primary senses / POS correction (482 words) | b84bd89, 1f774ec |
| 7 | OOV cascade blocker (scorer v3) | 5d6e0b8 |
| 10 | Mode-1 audio replay removed on correct | 3240c21 |
| 11 | Mode 2/3 detail page behavior fixed | 6e26ccc |
| 13 | Per-word progress verified already correct | — |
| 14 | Image_file migrated to examples (code) | d0b9fd8, 5a8148a |
| 15 | Unified review mode | d7af2f6 |
| 16 | Audio on wrong answer | 8ef630d |
| 18 | Image scoring ignores source; no auto-pin | 963b798 |
