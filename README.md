# Morpho

考研英语词汇 App。通过全英文释义学习单词——不看中文翻译，直接用英文理解英文。

---

# 第一部分 · 产品设计

## 核心机制

考研大纲约 5500 词。每个词配有英文释义，释义本身也由英文单词组成。把所有释义中出现的词提取出来，形成一张词与词之间的依赖图：要看懂 "benevolent" 的释义 "well meaning and kindly"，需要先认识 "meaning" 和 "kindly"。

将小学及初中英语教学大纲词汇（约 2000 词）标记为**基础词**（base），视为用户默认已知，不纳入学习范围也不参与依赖计算。考研大纲去掉基础词后，剩余词才是需要学习的**目标词**（target）。

释义文本中可能出现既非基础词也非目标词的**大纲外词**。这些词如果不处理，用户就读不懂释义，核心假设崩溃。处理方式：优先改写释义避开大纲外词（LLM 起草、人工确认，改写为只使用基础词 + 目标词的版本）；改写不了的，将该词提升为**辅助词**（auxiliary）——不是考研目标但用户必须先认识，参与拓扑排序，安排在引用它的目标词之前学习。辅助词与目标词走完全相同的资产构建和学习流程。

依赖图中存在循环依赖（A 的释义用了 B，B 的释义用了 A）。用 Tarjan 算法找出所有强连通分量（SCC），将循环依赖的词打包成一组同时学习。对整个图做拓扑排序，得到一条学习路径——保证用户学到任何一个词时，该词英文释义里用到的所有词，用户都已经学过或属于基础词。

这是整个产品的基础假设：按依赖顺序学习，让英文释义永远可读。

## 学习模式

百词斩模式 + 全英文。每组 15–20 词，三轮渐进检测。

**模式 1 — 句子 + 四图选择**：展示英文例句，目标词高亮，下方 4 张图片。用户根据语境选择对应图片。选对 → 下一词；选错 → 震动，显示英文释义辅助，必须选对后查看详情页。

**模式 2 — 单词 + 四图（带英文释义标签）**：只展示单词和音标，无例句。4 张图片各附一行英文释义。靠图片 + 释义匹配。

**模式 3 — 单词 + 四条英文释义（纯文字）**：只有单词和音标。4 条纯文字英文释义选项，无图无句，纯靠记忆。

**升降规则**：每个词从模式 1 开始。答对升一级，答错留在当前级。三轮结束后：三轮全对 → 标记已学，进入 FSRS 复习调度；未通关 → 保留当前模式等级，穿插进下一组继续学习。

**干扰项**：每个词预先绑定 3 个固定干扰项，在内容构建阶段一次确定，此后永不变更——无论第几次学、第几次复习，选项都是同一组，用户记住的是词和意思的对应关系而不是排除法。干扰项选取形态相近、容易混淆的词（如 adapt / adopt / adept），只从目标词和辅助词中选取（保证干扰项自身有完整资产），所有模式和复习共用：图片模式用干扰项的配图，释义模式用干扰项的主释义。

**详情页**：单词、音标、发音，按词性分条的全部释义（各带朗读），例句（带朗读，模式 1 用第 1 条，其余展示在详情页），词源与词根拆解。

## 复习

已学的词进入 FSRS v5 间隔重复调度。每天打开 App 先完成到期复习词，再开始新词学习。

复习题型两种：**释义选词**（给出主释义，从 4 个单词中选正确的，选项为该词 + 3 个固定干扰项）、**听力拼写**（播放单词发音，用户拼写单词）。按遗忘次数（lapses）加权选题型：遗忘多的偏向听力拼写，遗忘少的出释义选词。

## 用户体验

**首次使用**：直接开始学习。拓扑排序靠前的词本身就是较基础的词，考研生大多已认识，快速通过即标记已学——学习过程本身就是水平适应，无需单独测试。

**每日流程**：打开 App → 今日任务（N 个新词 + M 个到期复习）→ 先复习后新词 → 按组推进 → 达到当日量结束 → 今日总结。

**设置**：每日新词量可调，默认 50 词/天。

**进度展示**：已学 / 总词数进度条、今日已学 / 目标、连续学习天数、每组正确率。

---

# 第二部分 · 系统架构

## 总览

```
Morpho/
├── core/        # Rust：morphod 单二进制（对账引擎 + 管理 API + 导出器）
├── admin-ui/    # TypeScript：React + Vite 管理前端，构建产物由 morphod 静态托管
├── adapters/    # Python 适配器：tts / morfessor / sdxl / codex / clip（子进程）
├── app/         # Android：Kotlin + Jetpack Compose
└── data/        # working.db（SQLite WAL）+ 内容寻址媒体库
```

| 组件 | 选型 | 理由 |
|---|---|---|
| 核心服务 `morphod` | Rust：tokio + axum + rusqlite（SQLite WAL，单写入任务）+ reqwest | 一个进程独占工作库和对账循环；axum 在同一进程内服务管理 API，编辑和引擎共享同一事务视图，零 IPC；rusqlite 直连 SQLite，单写入线程 + WriteOp 通道让事务边界完全显式 |
| 管理前端 | TypeScript：React + Vite + TanStack Query/Router | Query 的缓存失效模型与"编辑写状态、引擎自动跟进"一一对应；静态文件单二进制部署 |
| Python 工具 | 薄 CLI 适配器，stdin JSON 请求 / stdout JSON 结果 | edge-tts、Morfessor、SDXL（diffusers）无成熟 Rust 等价物；适配器无状态、幂等，重试/限流/新旧判断全部留在 Rust |
| Android App | Kotlin + Compose + SQLDelight + Media3 + Coil，minSdk 26 | 离线、媒体密集、仅 Android 一个平台：Play Asset Delivery 是原生 Gradle 机制，音频要零拷贝 `AssetFileDescriptor` 播放，四图九宫格是 Compose 原生渲染热路径；无第二平台可摊薄 RN 桥或 Tauri webview 的成本 |

所有触图的逻辑——分词与词表分类、Tarjan SCC、拓扑排序、分组、干扰项绑定、出厂门槛、导出——全部在 Rust 内完成；WordNet 直接在进程内解析 WNdb 数据文件（释义兜底 + 语义相似度），不走子进程。只有叶端生成（TTS、形态切分、图片生成）外调。

## 设计原则

内容生产是一个**持续运行的对账系统**，没有阶段，也没有手动触发按钮。设计借用两个范式：

1. **Kubernetes 控制器 — 水平触发对账**。期望状态 = 每个目标词/辅助词都有完整、选定、通过校验的资产集。引擎随时可以做一次"全量扫描、与期望状态求差"来保证正确性；变更通知只降低延迟，不承担正确性。
2. **构建系统（ninja/bazel）— 计算出来的过时**。每个派生产物记录其精确输入的哈希。过时 = 记录哈希 ≠ 当前哈希，是一次比较，永远不是一次传播。没有任何代码"把下游标记为 stale"——输入一变，下游*就是*过时的，由读取方检测。

人只做编辑行为：批准、覆盖选择、处理大纲外词、重试死信。每个编辑行为都只是一次数据库写入，引擎自动收敛出全部后果。

## 数据来源

| 资产 | 来源 | 兜底 |
|---|---|---|
| 目标词表 | 考研大纲词汇表（词频排名、音标） | — |
| 基础词表 | 小学 + 初中英语教学大纲词汇表 | — |
| 释义候选 | Free Dictionary API（按词性分条） | WordNet 释义；LLM 改写（大纲外词规避）；人工 |
| 例句候选 | 考研大纲语料 | LLM；人工 |
| 图片候选 | Unsplash / Pexels / Pixabay API | Wikimedia / Openverse；SDXL 生成（本地 ComfyUI）；codex 生成；人工上传 |
| 图文语义匹配 | CLIP 子进程适配器（open_clip） | 无 —— 缺席即退化为纯画质排序 |
| 词源 | Wiktionary | Morfessor 形态学切分 |
| 发音音频 | edge-tts（单词、释义、例句各自独立合成） | — |

---

# 第三部分 · 数据模型（工作库）

工作库围绕三层组织：

1. **候选（candidates）** — 不可变行。抓取、生成、人工录入的内容一经写入永不修改；"编辑"= 铸造一个新候选。候选永不自动删除。
2. **选择（selections）** — 每个槽位一行可变记录，指向当前生效的候选。自动按评分选取，人可覆盖（pin）、可批准（approve）。
3. **派生（derived）** — 一切由**选定内容**计算出的东西（分词提取、TTS、学习计划、发布导出），各自携带输入哈希。引擎的全部工作就是：算期望状态 → 与记录哈希求差 → 生成任务。可以用 SQL 视图表达的派生物直接用视图——视图永远不会过时。

内部哈希统一 blake3，对规范化输入计算（NFC、去首尾空白、内部空白折叠；保留大小写——TTS 对大小写敏感）。每个哈希混入产生代码的 `algo_version`，工具升级自动精确失效自己的产物。

## 词表

```sql
words (
    word_id         INTEGER PRIMARY KEY,   -- 永不重编号；用户进度跨版本以此为键
    lemma           TEXT UNIQUE COLLATE NOCASE,
    role            TEXT,                  -- target / base / auxiliary
    aux_status      TEXT,                  -- 辅助词专用：active / retired
    phonetic        TEXT,
    frequency_rank  INTEGER,
    etymology       TEXT,
    etymology_source TEXT,                 -- wiktionary / morfessor / manual
    created_by      TEXT                   -- import / promotion / manual
)

-- 只有 active_words 参与资产生成、依赖边、排序和发布
VIEW active_words = target ∪ (auxiliary AND aux_status='active')
```

基础词与目标词同表，分词分类只需一次 join。注意工作库的 `words` 上**没有** `learning_order` / `group_id` 列——排序结果只存在于版本化的计划产物里（见下），不存在会过时的反规范化副本。

## 候选与选择

三类内容资产共用同一模式，字段大同小异：

```sql
definition_candidates (
    def_cand_id, word_id, pos,
    text, text_hash,                -- 不可变
    source,                        -- freedict / wordnet / llm_rewrite / manual
    parent_cand_id,                -- 改写血缘：LLM/人工改写链接到原候选，可审计、可回退
    status,                        -- available / rejected
    auto_score, score_detail, scorer_ver,
    UNIQUE (word_id, pos, text_hash)   -- 重复抓取自动去重
)

definition_selections (
    PRIMARY KEY (word_id, pos),    -- 每词每词性一个槽位
    def_cand_id,                   -- 当前生效候选
    is_primary,                    -- 每词恰好一个主义项（部分唯一索引强制），驱动全部题型
    enabled,                       -- 人可整槽禁用
    selected_by,                   -- auto / human
    pinned,                        -- 置顶后自动选择永不触碰
    approved, approved_hash, approved_by, approved_at,
    selection_rev                  -- 每次换候选自增
)

example_candidates ( ex_cand_id, word_id, text, text_hash,
    hl_start, hl_end,              -- 高亮区间属于这段确切文本，故放候选上
    source,                        -- exam_corpus / llm / manual
    status, auto_score, ... )

example_selections ( PRIMARY KEY (word_id, slot), slot 1..3, ... )
    -- slot 1 = 模式 1 用句；slot 2–3 展示在详情页

image_candidates ( img_cand_id, word_id, pos,
    file_hash,                     -- 图片字节 blake3；文件存 media/img/{hash}.webp
    source,                        -- unsplash / pexels / pixabay / wikimedia /
                                   -- openverse / sdxl / codex / manual
    source_ref,                    -- 图库 photo id / {prompt, seed, model} / 上传备注
    license, query_used, width, height,
    status, auto_score, ... )

-- 图文语义匹配：内容寻址，键是"比较了什么"，与候选无关
clip_scores ( PRIMARY KEY (file_hash, text_hash, model_ver),
    similarity,                    -- 两个单位向量的余弦
    computed_at )

image_selections ( word_id PRIMARY KEY, img_cand_id, ... )
    -- 每词恰好一张生效图：该图同时充当其他词题目里的干扰图，必须唯一
```

媒体文件**内容寻址**：`data/media/{hash[:2]}/{hash}.webp|.ogg`，同一张图被两个词抓到只存一份。`media_files` 表是磁盘真相的唯一登记处（file_hash, kind, rel_path, bytes, gc_eligible_at），GC 只对表操作，从不盲扫文件系统。

## 派生 · 分词与依赖（视图，永不过时）

关键设计：**分词按候选缓存，分类是视图**。分词开销大但只依赖候选的不可变文本，每候选每工具版本至多跑一次；把一个 token 判定为基础词/依赖/大纲外则依赖活的词表——所以它是一个 join，永远不会过时。提升一个辅助词、修订基础词表，全部零再生成、即时生效。

```sql
def_extractions ( def_cand_id PRIMARY KEY, input_hash, tokenizer_ver, lemmatizer_ver )
def_tokens ( def_cand_id, position, surface, lemma )   -- 屈折形还原为词元

-- 依赖边：只来自"当前选定"的释义。视图，读取时现算。
VIEW def_dependencies =
    definition_selections ⋈ def_tokens ⋈ words(role IN target,auxiliary)

-- 大纲外词出现表：同样是视图
VIEW oos_occurrences =
    definition_selections ⋈ def_tokens ⟕ words WHERE 无匹配词元
```

面向人的队列是引擎每周期与视图同步的实体表：

```sql
oos_queue ( oos_lemma PRIMARY KEY,
    status,        -- open / resolved_rewrite / resolved_promote / auto_closed
    first_seen, resolved_by, resolved_at, notes )
```

同步规则：视图中出现而队列没有的词元 → 插入 `open`；队列中 `open` 但视图已消失（改写被选中或词已提升）→ `auto_closed`。两种处理都是纯状态写入："改写" = 插入新 `llm_rewrite`/`manual` 候选并选中；"提升" = 插入 `role='auxiliary'` 的词行——新辅助词资产为零，自动产生全套抓取需求。

## 派生 · TTS（内容寻址）

TTS 以**合成了什么**为键，而不是哪个候选想要它。两个文本相同的候选共享一条音频；选择来回切换零成本；拒绝候选永不作废音频。

```sql
tts_assets (
    input_hash UNIQUE,   -- blake3(canonical(text) ‖ voice ‖ engine ‖ engine_ver ‖ params)
    text, text_hash, kind,          -- word / definition / example
    voice, engine, engine_ver, params_json,
    file_hash → media_files, duration_ms,
    status               -- ready / failed
)
```

期望集合是一个视图（`tts_desired`）：所有 active 词的词元 ∪ 所有选定释义文本 ∪ 所有选定例句文本，各自与当前语音配置组合出 input_hash。缺失或 failed 的哈希即成为合成任务。TTS 行从不"原地过时"：文本或引擎配置一变，产生的是*不同的* input_hash——新期望行出现，旧行失去引用，走 GC。

## 派生 · 学习计划（版本化）与干扰项（一次绑定）

```sql
plan_artifacts ( plan_id, input_hash, algo_ver, params_json,
                 is_current,        -- 部分唯一索引保证至多一个
                 built_at, stats_json )
plan_groups    ( plan_id, group_seq, group_type )   -- scc / root / semantic / fill
plan_words     ( plan_id, word_id, learning_order, group_seq )
```

计划输入哈希 = blake3(algo_ver ‖ 参数 ‖ 有序 active 词 id ‖ 有序依赖边集 ‖ 分组特征)。每词的依赖集哈希已在提取时物化，算全局哈希是一次索引扫描（约 1 ms）。哈希不符即重算（2 s 防抖合并编辑风暴）：Tarjan SCC → 凝聚图 → 拓扑排序 → 分组（SCC 打包 → 共享词根聚合 → WordNet 语义聚类 → 15–20 填充），单事务写入新计划产物。5500 节点、数万条边的规模下 O(V+E) 是个位数毫秒，改一条选定释义即可触发全量重排。所有排序平局按 (frequency_rank, word_id) 决定——相同输入产出逐字节相同的结果，`learning_order` 不会无谓抖动。旧计划保留（`is_current=0`）供管理界面 diff。

```sql
distractors ( PRIMARY KEY (word_id, rank), rank 1..3,
              distractor_word_id, algo_ver, bound_at, bound_by,
              CHECK 不指向自己, UNIQUE (word_id, distractor_word_id) )
```

产品规则：干扰项永不变。此表**故意豁免于哈希过时机制**——引擎只为缺少绑定的词插入（编辑距离最近的形态相近词，池 = target ∪ auxiliary），从不更新既有行。人工替换绑定是唯一的行变更路径，且记事件。干扰项引用同时构成辅助词的存活引用。

## 选择语义（精确规则）

自动选择每对账周期按槽位执行：

1. `scorer_ver` 落后的候选重评分。评分输入：可读性（大纲外 token 重罚）、长度窗口、来源先验（释义 manual > llm_rewrite > freedict > wordnet；例句 exam_corpus > llm）、词性/主义项匹配、分辨率（图片）。图片**不含来源先验**（scorer/4）：一张生成图和一张图库照片在分辨率与主义项匹配相同时完全同分。

   图片排序还有两项**不进入 auto_score** 的因素，因为它们依赖会变的状态，缓存进按 `scorer_ver` 失效的分数里会让一次无关编辑触发全库重评：
   - **语义贴切度**（占 0.60，主导项）：该图与本词 slot-1 例句的 CLIP 余弦，来自 `clip_scores`。整词全有或全无——池子里只要有一个候选没算过，整词回退到纯画质排序，否则等于拿两把尺子量。缺席语义分时的排序与该项引入前逐位一致。
   - **重复图惩罚**：全库范围内别的词已经选中的 `file_hash` 扣 0.10；同一道题内（干扰项互为题友）则直接**否决**——四选一里出现两张相同的图即不存在唯一正确答案，故为否决而非扣分。
2. 槽位无选择 → 指向得分最高的 available 候选，`selected_by='auto'`。
3. `auto` 且未 pin → 仅当别的候选超出当前候选一个**滞回边际 δ** 才切换（防评分接近时来回摆动）。切换即 `selection_rev++`、记事件。
4. `pinned=1` → 自动选择永不触碰。**例外**：被 pin 的候选被拒绝或其词离开 active_words 时，引擎清除 pin、按规则 2/3 回退、重置 `approved=0`、写事件进管理收件箱。
5. 主义项：首次选择时按词频证据最强的词性标 `is_primary`；人可移动，移动受部分唯一索引事务保护。

**人工覆盖** = 把选择行改指另一候选（`selected_by='human', pinned=1`），或先铸造 manual 候选再选中。就是一次状态写入——引擎看到新 `selection_rev`，哈希机制处理其余一切。不存在"重跑某阶段"按钮。

**批准**属于 (槽位, 候选, 内容) 三元组：`approved=1` 时记录 `approved_hash`（候选的 text_hash/file_hash）、批准人、时间，并隐含 pin。批准被作废（重置为 0、记事件）当且仅当：槽位所指候选变化（人工切换、pin 回退），或 `approved_hash` 与所选候选哈希不再相等（防御性检查；候选不可变时不可达），或管理员显式批量撤批（编辑规范修订时）。除此之外没有任何事作废批准——计划重建、TTS 重合成都在已批准内容的*下游*，不碰批准。

## 任务、事件、就绪度

```sql
job_state (      -- 队列不持久化：QUEUED/RUNNING 只存在于内存（InFlight 集合），
                 -- 此表只记需要跨重启记住的失败态；成功即删行，无行即健康
    PRIMARY KEY (kind, subject_type, subject_id),
    kind,            -- fetch_definitions / fetch_examples / fetch_etymology / fetch_images
                     -- score_image_clip / gen_image_sdxl / gen_image_codex
                     -- rewrite_definition / synth_tts / ...
    subject_type, subject_id,
    rate_key,        -- freedict / unsplash / pexels / pixabay / wikimedia / openverse
                     -- tatoeba / sdxl / clip / codex / edge_tts / llm / cpu
    status,          -- backoff / dead / waived
    attempts, next_retry_at, last_error
)

source_fetch (   -- 完成标记：合法零结果的抓取也记录，期望规则查标记而非查候选存在性
    PRIMARY KEY (kind, word_id, source),
    fetched_at, result_count
)

rate_limits ( rate_key PRIMARY KEY, max_concurrency, refill_per_min, burst )

events (     -- 只追加审计日志
    ts, actor,       -- reconciler / worker:<kind> / admin:<user>
    entity_type, entity_id,
    action,          -- candidate_added / selection_changed / approved / approval_invalidated
                     -- pin_fallback / aux_promoted / aux_retired / plan_rebuilt
                     -- distractor_bound / job_dead / release_exported / ...
    detail           -- JSON before/after 快照
)
```

每次选择变更、批准翻转、提升/退役、计划重建、导出都写事件。"人工编辑只是状态变更"之所以安全，就在于永远可以重构出引擎为什么做了某件事。

**就绪度是纯 DB 数学，不是任务**，引擎每周期内联重算：

```
core_ready(W) =  主义项已选且已批准
              ∧ 每个 enabled 义项均已批准
              ∧ 选定释义无大纲外 token、依赖全部被 基础词 ∪ 拓扑前序词 覆盖
              ∧ slot 1 例句已选已批准 ∧ 生效图片已批准
              ∧ 词/各选定释义/选定例句的 TTS 全部 ready

ready(W)      =  core_ready(W)
              ∧ 3 个干扰项已绑定
              ∧ 每个干扰项自身 core_ready
```

把 `core_ready` 与 `ready` 拆开将递归深度封在 1：互为干扰项（adapt ↔ adopt）不会死锁——干扰项只需自己的核心资产，不需要它自己的干扰项。未就绪原因物化为 blocker 列表（`missing_image`、`oos_pending`、`distractor_2_not_ready`…）供仪表盘展示。

## 辅助词生命周期与媒体 GC

```
VIEW aux_liveness = 从目标词出发可达：沿存活词的已启用释义依赖边或干扰项绑定传递
           （已退役词的引用、无存活入口的辅助词环都不算）
```

对账规则：active 且不再被引用 → `aux_status='retired'`（离开 active_words、计划、tts_desired 和发布，候选/选择/媒体全部原地保留）；retired 且重新被引用 → `active`，资产完好归队。全程可逆，永不删除。

媒体 GC：`gc_media` 任务周期性计算引用集 = 候选引用 ∪ TTS 引用 ∪ **发布清单引用**。引用集之外的文件记 `gc_eligible_at = now + 14 天`，宽限期后仍无引用的移入回收站（永不 `rm`）。候选行本身从不被 GC——只有显式的管理员清除会删候选行。已发布 APK 引用过的文件被 release_manifests 永久钉住。

## 哈希覆盖一览

| 派生物 | 键 | input_hash 覆盖 | 何时重算 |
|---|---|---|---|
| 分词提取 | 释义候选 | text_hash ‖ tokenizer_ver ‖ lemmatizer_ver | 仅工具版本升级（候选不可变） |
| 依赖边 / 大纲外出现 | — | 无 —— **纯视图** | 永不；读取时现算，词表变更零成本即时生效 |
| 候选评分 | 候选 | scorer_ver（其余输入不可变或为视图） | scorer 升级；辅助词提升后的廉价批量重评 |
| 图文语义分 | (图片哈希, 文本哈希, model_ver)（内容寻址） | 键即全部输入 | 从不原地重算——换模型/换算法写的是**新行**，旧行失去读者。文本一变（换了 slot-1 例句）也只是查一个不存在的键，缺席即退化 |
| TTS | input_hash（内容寻址） | canonical(text) ‖ voice ‖ engine ‖ engine_ver ‖ params | 从不原地重算——期望集合差分产生新行，孤儿走 GC |
| 依赖提取哈希 H_dep | 释义候选 | text_hash ‖ sorted[(token, 分类)] ‖ ver | 按**出现 token** 记分类：提升一个大纲外词只失效包含它的那几条释义，绝不失效全部 5500 条 |
| 学习计划 | 全局单例 | algo_ver ‖ 参数 ‖ 有序词集 ‖ 有序边集 ‖ 分组特征 | 哈希不符（2 s 防抖），毫秒级重算 |
| 干扰项 | (word, rank) | 故意无 | 永不（产品规则），只补缺 |
| 发布导出 | release | 计划哈希 ‖ 就绪词的全部选定内容/媒体哈希 ‖ 导出 schema 版本 | 哈希变化即自动构建发布候选；发布 APK 始终是人的行为 |

普适不变量：派生行记录影响其字节的一切输入的哈希；引擎从活状态重算当前哈希并求差。**变更方从不设置任何标志——由读取方检测变化。** 这让人工编辑、自动选择翻转、工具升级走完全同一条路径。

---

# 第四部分 · 对账引擎（morphod）

## 组件

单二进制、单进程，组件皆为 tokio 任务，经通道通信：

| 组件 | 职责 |
|---|---|
| **Store** | 独占 SQLite。一个写入任务（唯一写连接）+ 4–8 只读连接池。一切变更以类型化 `WriteOp` 走写入任务的 mpsc 通道，oneshot 回执 |
| **Change Bus** | `broadcast<ChangeEvent>`。写入任务在每个事务提交后发布被触及的实体键。仅作边沿触发加速 |
| **Reconciler** | 核心循环。被唤醒（事件/定时/启动）→ 取读快照 → 跑期望状态规则 → 与在途任务及 job_state（退避/死信/豁免）求差 → 发新任务给 Dispatcher；同时内联重算就绪度与 blocker |
| **Dispatcher** | 每外部源一条泳道（freedict、wiktionary、unsplash、pexels、pixabay、wikimedia、openverse、tatoeba、llm、edge_tts、sdxl、clip、codex）+ 本地 `cpu` 泳道（提取、评分、图重算、干扰项、WordNet 查询）。每泳道：governor 令牌桶 + 并发信号量 + 工作任务 |
| **Executors** | 执行单个任务：调适配器 → 类型化结果 → 提交**单个原子** WriteOp（结果行与记录的输入哈希同事务）。执行器不持有 DB 连接 |
| **Adapters** | trait 对象封装外部工具。HTTP 走 reqwest；子进程走 `tokio::process`（kill_on_drop + 硬超时）；WordNet 为进程内内存数据。适配器是纯函数：类型化输入 → 类型化输出，不见数据库 |
| **Admin API** | 同进程 axum 路由，服务 TS 管理前端。每个变更端点只是到 WriteOp 的薄翻译——因此每个编辑行为天然触发 ChangeEvent。管理端与引擎之间不存在第二条通道 |
| **Janitor** | 启动清理临时目录、媒体 GC、周期 `wal_checkpoint(TRUNCATE)`、指标落盘 |

SQLite：`WAL + synchronous=NORMAL + foreign_keys=ON + busy_timeout=5000`。单写入任务使自身各次写入之间结构性不会出现 `SQLITE_BUSY`；跨进程写者不存在，因为管理端就在进程内。

媒体写入：先写临时文件 → 哈希 → 原子重命名进内容寻址库 → 再提交 DB 行。相同内容的再生成是一次空操作重命名；崩溃只留下临时文件，Janitor 开机清理。

## 对账循环

唤醒源按权威性排序：

1. **启动**：全量推导。仅此一条即保证任何崩溃、部署、离线改库后的收敛。
2. **定时**：每 60 s 全量兜底。事件丢失最多付出一分钟延迟。
3. **变更事件**：250 ms 合并窗（吸收批量编辑），然后只跑触及实体相关规则的**局部推导**。局部只是优化——全量永远会产出同一任务集的超集。

幂等性由结构保证：抓取结果按 `(word_id, source, source_ref)` upsert，重抓不重复；派生产物与其 input_hash 同事务 upsert，不存在"产物在、来历不在"的窗口；全局重算用乐观并发——基于快照计算，WriteOp 携带计算时的输入哈希，写入任务在事务内复核当前哈希，输入已漂移则丢弃结果，下个周期自然重来。

另有一类**完成标记**（`source_fetch`）：合法返回零结果的抓取也记标记，期望规则查标记而非查候选是否存在——空结果不会造成无限重抓。

## 任务生命周期

**队列是推导出来的，不是存储的。** 持久化队列会制造与 DB 不一致的第二个真相源。

- `QUEUED`/`RUNNING` 仅存在于内存（InFlight 集合去重）；崩溃丢失它们无代价——重启全量推导会重新发现一切未满足的需求。
- 失败 → `job_state` upsert：`attempts++`，`next_retry_at = now + min(30s·2^attempts, 1h) ± 20% 抖动`。推导跳过未到重试时刻的主体。成功**删除**该行——无行即健康。
- 超过每类上限（HTTP 抓取 8、TTS 5、SDXL 3、LLM 4）→ `dead`。死信从推导中排除，浮出到管理端**死信箱**（联表词条、错误、尝试史）。人的操作同样只是 DB 写：**重试** = 删掉 job_state 行（需求即刻重新推导出）；**豁免（waive）** = "此需求由缺席永久满足"，也是兜底规则的触发条件：Wiktionary 被豁免 → 走 Morfessor；三个图库全部标记/死信/豁免且无候选无人工图 → 走 SDXL。
- 错误分类：`Permanent`（404 等合法空结果——记完成标记，不重试）/ `Transient`（网络、5xx、超时——退避）/ `RateLimited{until}`（整条泳道停靠到指定时刻，不计尝试次数）。

优先级（小者先，平局按 frequency_rank 再 id，完全确定）：**P0** 解锁一切的廉价本地计算（提取、选择、图重算）；**P1** 编辑作废资产的再生成、阻塞待发布的一切；**P2** 存量回填（初始抓取、TTS、干扰项），按学习顺序排——最早的组最先变得可发布；**P3** 昂贵生成兜底（SDXL、codex、LLM 改写）。

## 适配器

```rust
trait DefinitionSource { async fn fetch(&self, word: &str) -> Result<Vec<DefCandidate>, AdapterError>; }
trait ExampleSource; trait EtymologySource; trait ImageSource;
trait ImageGenerator; trait TtsEngine; trait Segmenter;

enum AdapterError { Permanent(String), Transient(String), RateLimited { until: Instant } }
```

| 工具 | 机制 |
|---|---|
| Free Dictionary API | HTTP（reqwest），按词性 JSON → 候选 |
| WordNet 3.1 | **进程内原生**：启动时把 WNdb 数据文件解析进内存映射；gloss 作释义兜底候选，synset 相似度供语义分组。无子进程、无延迟 |
| Wiktionary | HTTP REST，词源抽取 |
| LLM 改写 | HTTP（OpenAI 兼容端点）；prompt 以版本哈希钉住；产出重分词复检，仍含大纲外 token 则拒绝（= Permanent，浮给人） |
| Unsplash / Pexels / Pixabay | HTTP；元数据 + 下载字节 → 内容寻址库 |
| SDXL | HTTP 到本地 ComfyUI（POST /prompt，轮询 /history）；泳道并发 1，超时 10 min |
| CLIP | 子进程（`adapters/clip`），stdin/stdout JSON 信封。引擎传 `media_root`，适配器自行解析 `{root}/{hash[:2]}/{hash}.webp`。每任务一个进程，模型加载 ~2-4 s（CPU），打分毫秒级。Docker 内 CPU 推理；原生运行时自动使用 GPU。契约见 `docs/contracts/clip-subprocess.md` |
| codex | 子进程（`adapters/codex`），把词的 slot-1 例句交给外部图像生成器；超时 15 min。适配器项目不在盘上、或 `MORPHO_CODEX_BIN` 找不到二进制 → 源**禁用**（同"没有 API key"），绝不死信刷屏 |
| edge-tts | 子进程（adapters/tts），60 s 超时，stderr 进 last_error |
| Morfessor | 子进程，stdin/stdout 批处理行协议，一次进程摊薄一批词 |

## 联动示例（三种编辑，一套机制）

**A. 编辑 benevolent 的选定释义文本。** 管理端一次 WriteOp：铸造 manual 候选 + 选择改指（human）。下个周期：H_tts 不符 → TTS 任务（P1）；H_dep 不符 → 提取任务（P0）即刻重跑，重写该释义的依赖边与大纲外标记。若编辑引入了 "altruistic"（大纲外）→ oos 队列出现新条目、该词 ready 翻 false（blocker: oos_pending），同时 LLM 改写任务（P3）已在排队——人打开队列时规避草稿已经躺在候选里。依赖集变化 → H_graph 变化 → 防抖后计划重建。TTS 完成、大纲外词处理完，就绪度自动翻回。全程无人触发任何"阶段"。

**B. 拒绝 abandon 的选定图片。** 一次 WriteOp：候选置 rejected。下周期：选择指向被拒候选 → 自动选择（P0）从余下候选重选；无候选可选 → 查完成标记，未查过的图库 → 抓取任务（P2）；图库全部无果 → 二次宽松检索 → SDXL（P3）→ codex（P3）。期间 abandon ready=false（missing_image），**所有把 abandon 当干扰项的词**同时翻 false（distractor_not_ready）——因为就绪度是对当前状态的派生数学，没有任何代码需要知道反向干扰边的存在。

图片链条的最后一环的触发条件是**"不贴切"**，而非前面各环的"没有"。一个词完全可能拥有一池子清晰、授权干净、却画的是别的东西的照片——`adapt` 拿到的是电源适配器的棚拍图。这件事只有 CLIP 能观测到，所以 codex 源读的是"本词最好的一张图对着自己例句的分数低于阈值"。它必须以 slot-1 例句为条件生成：模式 1 就是让学习者拿句子对图，裁决生成结果的 CLIP 查询用的也是这个句子，换用别的条件会让生成与评判的目标脱钩。**没有例句的词直接推迟**，等例句落地后的下一个周期自然合格，无需清任何标记。

**C. 把大纲外词 serene 处理为辅助词。** 一个事务：插入 auxiliary 词行 + oos 行置 resolved。两条独立涟漪：新词零资产 → 全套抓取 fan-out（P2，各泳道限流下）；**所有包含 serene 这个 token 的释义**的 H_dep 改变（该 token 分类 oov → auxiliary）——按出现 token 记哈希的设计让爆炸半径精确到这几条释义。serene 自己的释义到位、选定、提取后，H_graph 变化 → 计划把它排在所有引用它的词**之前**，TTS/图片/干扰项照目标词一样流过。引用它的词保持未就绪（blocker：依赖未就绪）直到辅助词建满——然后就绪度在一次校验扫描里级联翻绿。

---

# 第五部分 · 发布构建

## 出厂门槛（按词）

词 W 可出厂（shippable）当且仅当（全部为哈希比较，无手动标志）：

1. 主义项已选已批准；每个 enabled 义项文本均已批准
2. 每条选定释义的提取是新鲜的，token 集内**零个未处理大纲外词**
3. slot 1 例句已选已批准，高亮区间有效（slot 2–3 可选，但凡选定必须已批准）
4. 生效图片已批准
5. 词、每条选定释义、每条选定例句的 TTS 齐备且新鲜
6. 3 个干扰项已绑定
7. W 在当前计划中有拓扑位置与分组

批准跟着候选走且有粘性：重新选回曾批准的候选无需再批；改文本自动重置批准，并经哈希自动使其 TTS 与提取过时。

## 依赖闭合裁剪

可出厂词的集合直接发布是不行的：某词的释义依赖被扣下的词，可读性不变量即破；某词的干扰项被扣下，选项资产即缺。导出器计算**最大依赖闭合子集**：

对 target/auxiliary 词建闭包图 G，W → X 的边来自两处：W 选定释义的依赖词（基础词除外）、W 的 3 个干扰项。然后跑不动点：

```
R ← { 全部可出厂词 }
repeat: 移除任何存在边 W→X 且 X ∉ R 的 W ∈ R
until 无可移除
```

性质（全部有意为之）：**SCC 整体进出**——释义环里一员不合格，环边把整个 SCC 移出 R，这是正确的，因为该组本就作为整体学习；互为干扰项同理，无特例；结果是对两类边闭合的唯一最大集，与移除顺序无关，反向边工作表 O(V+E)；R 内的学习顺序就是全局拓扑序在 R 上的限制——R 对释义依赖闭合，故所有前驱都在集合内，序仍然有效；15–20 词的分组在 R 上于导出时重切，组里永远没有洞。

导出器同时产出**扣留报告**：每个被排除的词，根因（哪条门槛失败、或传递依赖了哪个被扣词）+ 下游影响数（它阻塞了多少本可出厂的词）。管理端按影响数排序编辑工单，人先修阻塞面最大的词。

## 确定性导出

`morphod export` 从给定工作库状态产出逐字节可复现的包：

- 媒体按内容寻址名复制：`media/{hash}.webp / .ogg`；release.db 只以哈希名引用资产，去重免费
- release.db 每次全新构建：固定 page_size=4096、journal=DELETE（只读产物）、按 id 有序插入、结尾 VACUUM、零时间戳
- manifest.json：每个文件的大小与哈希 + 导出器 git 版本
- `content_version = YYYY.MM.DD+<manifest哈希前8位>`——相同输入、相同字节、相同版本；manifest 哈希就是发布身份
- 发布清单（release_manifests）钉住该版本用到的每个媒体文件，GC 永远删不到已发布 APK 引用的东西

`word_id` 在工作库分配一次、永不复用重编——用户进度跨任何内容版本存活。后续版本删掉的词留下的孤儿进度行，App 无害忽略。引擎在发布输入哈希变化且全部校验门通过时自动构建**发布候选**；把 APK 发出去始终是人的行为。

## release.db（APK 内只读）

只含 `ready=1` ∧ 依赖闭合裁剪后的词，按当前计划排序。基础词、全部候选表、选择元数据（评分/pin/批准/修订）、提取表、oos 队列、jobs、events、计划历史——全部剥离。

```sql
words       ( word_id PK, word, phonetic, frequency_rank, role,   -- target/auxiliary
              group_id, learning_order, etymology, image_file, word_audio_file )
senses      ( sense_id PK, word_id, pos, definition, is_primary, def_audio_file )
examples    ( example_id PK, word_id, display_order,   -- 1 = 模式 1 用句
              sentence, hl_start, hl_end, ex_audio_file )
groups      ( group_id PK, group_order, group_type )
distractors ( PRIMARY KEY (word_id, rank), distractor_word_id )
meta        ( key PK, value )   -- content_version, plan_id, exported_at, schema_ver
```

导出校验器（硬门，全过才出）：每个导出词 ready=1；每个 distractor_word_id 解析到导出词；无释义含大纲外 token；拓扑不变量成立（每条依赖边的前驱 learning_order 更小，或同 SCC 同组）；每个被引用媒体文件在 manifest 里；每词恰好一个主义项。

## 体积与打包

体积不是约束，质量优先。预算上限取 install-time Play Asset Delivery 的硬限制（单资产包 ≤ 2 GB，install-time 合计 ≤ 4 GB），实际用量远在其下。约 5500 目标词 + 约 500 辅助词 ≈ 6000 词；平均 1.5 条选定释义、每词最多 3 条选定例句全部带朗读。

| 资产 | 规格 | 单均 | 数量 | 小计 |
|---|---|---|---|---|
| 单词音频 | Opus mono 48 kbps（听写题需要清晰音素） | 6 KB | 6 000 | 36 MB |
| 释义音频 | Opus mono 32 kbps | 16 KB | 9 000 | 144 MB |
| 例句音频 | Opus mono 32 kbps | 24 KB | 18 000 | 432 MB |
| 图片 | WebP 768×576 q80（高分屏四图格无涂抹感） | 45 KB | 6 000 | 270 MB |
| release.db | — | — | — | 10 MB |
| 代码 + Compose + Media3 | — | — | — | 20 MB |
| **整机安装** | | | | **≈ 0.9 GB** |

打包：**AAB + install-time Play Asset Delivery**。基础模块带代码和 release.db（首帧就要）；全部媒体进一个 install-time 资产包——对用户就是一次普通的完全离线安装，无下载页、装后无网络依赖；距 2 GB 单包上限仍有一倍余量，可容纳第二配图或更多例句。媒体在包内**不压缩存储**（Opus/WebP 已压缩），允许 Media3 走 `AssetFileDescriptor` 零拷贝播放、WebP 直接解码，无解包步骤、无双倍磁盘。转码用钉版本的 libopus 固定参数，保证哈希稳定。

Play 之外的分发：第二个 Gradle flavor `fatApk` 把同一媒体目录嵌进 `assets/` 出整包 APK（约 1 GB，远低于 zip 的 4 GB 限制），供直接下载。两种分发共用代码路径——`ContentStore` 抽象把哈希文件名解析成流，两边实现各异、接口相同。

---

# 第六部分 · App 设计

Kotlin + Jetpack Compose，minSdk 26。三层架构：

```
UI 层（Compose）
├── 学习界面（模式 1/2/3、详情页）
├── 复习界面（释义选词、听力拼写）
├── 主页（进度、今日任务）
└── 设置

Domain 层
├── LearningEngine    # 会话构建：到期复习优先，然后按 learning_order 取未学词
│                     # （组是呈现单位，不是进度单位，见"升级与进度保留"）
│                     # 模式升降、三轮判定、未通关词穿插下一组
├── ReviewScheduler   # FSRS v5：到期计算、按 lapses 加权选题型、卡片状态持久化
└── ProgressTracker   # 全部计数派生自 user.db；分母来自 release.db——
                      # 内容更新增词后进度条自动适应，零迁移

Data 层
├── ContentStore       # 哈希文件名 → InputStream / AssetFileDescriptor
│                      #   PAD flavor：AssetManager 读 content_media 资产包
│                      #   fatApk flavor：直接读 assets/
├── ContentRepository  # SQLDelight over release.db（只读打开）：
│                      #   词、选定释义、例句、分组、干扰项，皆按 learning_order
├── ProgressRepository # SQLDelight over user.db（读写）
├── AudioPlayer        # Media3：主播放器 + 一个预载槽，渲染当前题时预载下一题音频
└── ImageLoader        # Coil + ContentStore fetcher，网络栈不启用
```

**两库皆用 SQLDelight**：release.db 由外部工具产出，Room 的 schema 所有权校验和迁移机制在此只是额外开销；SQLDelight 对着导出 DDL 的 `.sq` 文件编译类型化查询，不要求运行时拥有 schema。一套工具链、一种查询风格、全程 coroutines Flow。

**干扰项保证**：发布裁剪对干扰边闭合，所以引擎为每题一次 join 加载词 + 3 个固定干扰项时，**每个干扰项必然带着图、释义、音频在场**——引擎不携带任何回退路径，只在 debug 构建启动时跑一次全量断言扫描。

**每题的音频归属**：模式 1 揭示后播例句朗读；模式 2/3 播单词发音；详情页单独暴露词/各义项/例句的朗读；听写题循环点播单词发音。Opus 文件小，预载开销可忽略。

## 升级与进度保留

进度保留的全部根基是三条不变量：**word_id 在工作库分配一次、永不复用重编**；**进度只按词记录**（learning_progress / fsrs_cards 均以 word_id 为主键，不存 group_id、不存 learning_order）；**进度行永不删除**。组和顺序是每个内容版本的呈现产物，词的掌握状态才是进度本身。

### 覆盖安装（同签名升级）

user.db 存放在 App 私有数据目录，覆盖安装（Play 更新或 fatApk 直接装新版）只替换 APK 与资产包，私有目录原封不动——进度天然存活，PAD 和 fatApk 两种分发同理。release.db 是只读资产，每版整体替换，无迁移。

### 内容版本对账

首次启动时 App 比较 user.db 与 release.db 的 content_version，不同则执行一次**进度对账**（纯读写 user.db 的 meta，非破坏性）：

- **新增词**（release 有、进度无）：自然处于未学状态，按新版拓扑位置进入新词队列。新插入的前置词（如新提升的辅助词）拓扑位置必然靠前，会在后续新词学习中被优先遇到——已学词的释义引入新依赖时，缺口按顺序自动补上。
- **消失词**（进度有、release 无——被依赖闭合裁剪扣留或被退役）：进度行**保留不删**，仅通过与 release.db 的 join 自然退出复习队列和统计分母。该词在后续版本回归时，FSRS 状态原地续用（elapsed_days 会偏大，FSRS 本就为长间隔设计，无需特殊处理）。
- **释义/图片/例句变更的已学词**：进度与卡片状态不动——用户掌握的是词本身，资产修订不作废记忆；变更后的内容在下次复习时自然呈现。
- 对账结束记录新 content_version。全程无 schema 迁移、无进度重算。

### 组的短暂性

每个版本的分组在导出时对发布集重切，组边界跨版本必然漂移。因此 LearningEngine 从不持有"学到第几组"的指针：会话构建 = 按当前版本 learning_order 扫描未学词，取足当日配额，按其所属组边界切成 15–20 词的学习单元；组内已学的词直接跳过，残组与后续词自然合并。用户在任何内容版本上都是"从未学的最早的词继续"，升级前后衔接一致。

### 备份与迁移

无账号、无服务器，进度必须能离开这台设备：

- **Android Auto Backup**：user.db 只有数百 KB，远在 25 MB 配额内。声明 backup rules 包含 user.db（备份前 checkpoint WAL），云备份与设备间迁移（D2D）自动覆盖卸载重装和换机场景。
- **手动导出/导入**：设置页提供进度文件导出（user.db 快照经 SAF 写到用户选定位置）与导入。导入时校验 schema_ver 并走同一套内容版本对账，允许跨 App 版本恢复。

## 用户数据库（user.db）

```sql
learning_progress ( word_id PK, current_mode, rounds_passed, status )  -- learning / learned
fsrs_cards ( word_id PK, due, stability, difficulty, elapsed_days,
             scheduled_days, reps, lapses, state, last_review )
daily_stats ( date PK, new_learned, reviewed, correct_rate )
meta ( key PK, value )   -- content_version, schema_ver, daily_goal
```

## License

Source-available for noncommercial use only. Code: [PolyForm Noncommercial 1.0.0](LICENSE).
Content written for Morpho: [CC BY-NC-SA 4.0](content/LICENSE). Third-party text in the
shipped word database (Tatoeba, Wiktionary, WordNet and others) keeps its own licence and is
credited row by row; see [NOTICE.md](NOTICE.md). Pictures and audio are not in the repository.
