# 编排者操作参考

主文件放规范，本文件放操作细节。均以 docs/OPERATIONS.md 为最终权威；此处只记编排层面反复用到的事实。

## 发布链（wave-2 实测顺序）

1. 合并全部代码分支 → 复跑门禁。
2. `docker compose build` + 带环境变量 `up -d --force-recreate`（`MORPHO_CLIP_URL=http://host.docker.internal:30013`、`MORPHO_CLIP_MODEL=ViT-B-32/laion2b_s34b_b79k`）。宿主侧先起 CLIP sidecar（OPERATIONS.md §2.2），`/health` 须报 `clip/1` 且模型一致。
3. 等 `clip_scores` 灌满（涨停即满）→ `MORPHO_API=http://127.0.0.1:30012/api python3 ops/unapprove_auto.py image` 释放 pin → 重选收敛（selection_changed 事件涨停）。
4. 干扰项修复：`POST /api/distractors/rebind-violations` 先 `{"dry_run":true}` 审清单再 apply，复扫确认残留仅 unresolvable。
5. `MORPHO_API=http://127.0.0.1:30012 python3 ops/bulk_approve.py` → 就绪度收敛 → `GET /api/releases/preview` 过门（exportable>0、gate_failures 空）。
6. `docker compose stop` → 宿主 `MORPHOD_ADAPTERS_ROOT=<repo>/adapters MORPHO_WORDNET_DIR=<repo>/data/wordnet/dict morphod publish --no-build`（#29 修复前必须显式传 ADAPTERS_ROOT）→ `docker compose up -d`。
7. 核对媒体同步（manifest 零缺失；警惕根级平铺旧布局遗留，publish 的 stale 清理扫不到——#29 之二）。
8. 更新 ReleaseDatabaseTest 计数断言（agent 执行）→ gradle 全门 + assembleFatApkDebug。
9. APK：`app/app/build/outputs/apk/fatApk/debug/app-fatApk-debug.apk`。

## adb 装机

- 包名 `dev.morpho.debug`（CLAUDE.md 写的 dev.morpho 是应用 id 基名，卸载要用带后缀的真名）。
- overlay 覆盖装会损坏 App：装前 `adb uninstall dev.morpho.debug`；用 dumpsys 的 firstInstallTime==lastUpdateTime 验证干净装。
- 保进度流程：`run-as dev.morpho.debug cat databases/user.db*` 拉出 → 本地 `PRAGMA wal_checkpoint(TRUNCATE)` + `VACUUM` 合成单文件 → 卸载 → 装 → 首启前 `adb push` + `run-as cp` 灌回。
- #37（覆盖安装正式支持）落地后本节简化。

## Docker 陷阱

- `.dockerignore` 必须含 `.claude/`——agent worktree 的 core/target 可达数十 GB，漏掉会把构建卡死在上下文打包。
- 容器每次启动报一条 `adapter unavailable adapter="sdxl"` WARN 属预期（镜像只打包 tts/morfessor；SDXL/CLIP 走宿主）。
- compose 的数据卷是 bind mount，重建镜像不伤 working.db；仍应构建前后核对 dashboard 词数。

## 引擎已知缺陷（入册待修）

- #29 publish：repo_root 解析错一级（用 MORPHOD_ADAPTERS_ROOT 绕过）；stale 媒体清理不扫根级旧布局。
- #31 reject 在选候选后选择行可能滞留 `selected_by=human` 悬挂在 rejected 候选上，自动回退不接手，需人工改选。
- 画廊 source 筛选：codex 生成图经上传 API 入库挂 `manual` 标签；`codex` 标签仅引擎内建生成源（`MORPHO_CODEX_ENABLED`）产出。

## ops 脚本约定

- `unapprove_auto.py`：`MORPHO_API` 需带 `/api` 后缀；`bulk_approve.py`：不带。统一化在待办清单。
- CLIP 相关脚本用 `~/Code/vendor/ComfyUI/.venv/bin/python`。
- 图库健康快查（在选图 vs 例句语义分分布）：clip_scores 按词的 slot-1 text_hash 联查，<0.08 跑题、0.08–0.15 弱匹配。
