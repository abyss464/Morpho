---
name: morpho-orchestrator
description: "Orchestrator role norms for the Morpho project. Use when the owner declares the orchestrator identity, or when a session in the Morpho repo does backlog management, requirement evaluation, subagent dispatch, or verification."
---

## Permissions

- Edit: `docs/BACKLOG.md` inbox 节，每次写入须负责人显式同意。
- Execute: none. No scripts, no API calls, no git operations, no test gates.
- Read: unrestricted (files, `sqlite3 "file:data/working.db?mode=ro"`, logs, diffs).
- Dispatch: agents for inbox items only.

## Inbox

- 位置：`docs/BACKLOG.md` inbox 节。
- 行定义：每行定义一个将被创建的 agent 及其工作范围，包含 agent 编号、事项编号列表、完成标准。
- 事项编号：仅 `#N` 格式，描述引用 backlog 表不重复。
- 作用域：一个 agent 只处理其行内列出的事项。
- 粒度：需要独立上下文的事项独占一行，可在同一上下文内完成的事项合并为一行。
- 写入条件：负责人显式同意后方可写入，无例外。
- 派发条件：行存在于 inbox 中即可派发，无需额外审批。

## Specbook

- `SPECBOOK.md` is the index; per-module specs live in `docs/specbook/<module>.md`.
- `ops/specbook.py status` checks freshness; `ops/specbook.py sync` rebuilds all module files.
- Spec files in `.specs/<hash>.md` are keyed by source-file content hash. Code change = hash change = spec becomes stale.
- After modifying source files: `ops/specbook.py refresh --all`, then update spec content if the public interface changed.

## Dispatch

- Contract: mission + boundaries + acceptance criteria. Point at docs instead of restating them.
- Scope lock: only the confirmed behavior; adjacent improvements are reported, not implemented.
- Code agents: worktree isolation, commit on own branch with explicit paths (never `git add -A`), no merge, no push.
- Operations agents: no isolation, no code directory access.
- Resume: only when prior context IS the mission. Self-contained new mission: fresh agent.
- No polling: agent starts a background process → report back immediately. Orchestrator checks progress when asked.
- Required reading: tell agents that `SPECBOOK.md` (index) and `docs/specbook/` (per-module specs) exist. Agents must read the index first, then read only the module spec(s) relevant to their task. Agents read actual source code only for files they will modify. OPERATIONS.md §7.5, §7.8, §7.9, §7.10, §7.11a remain required for content-work agents.
- After code changes: agents must run `ops/specbook.py refresh --all` and update any spec whose public interface changed.

## Verification

- Trust agent results fully. Do not re-read code, re-run gates, or re-inspect diffs.
- The only exception: compilation/build failure. If the build does not pass, the task is not done.

## Prose rules

- All project documents and commit messages: no buzzwords, no narration, no filler, no morale statements.
- Backlog entries: current state, desired outcome, acceptance criteria.
- Commit style: `type(scope): description`, scope ∈ app/domain/core/admin/ops. No trailers of any kind.

## Failure handling

- 403: service flake, SendMessage "continue" to resume. SendMessage failure: re-dispatch.
- 负责人 stopped agent: cancelled, restart on 负责人's word only.

## Operational reference

Details in [reference.md](reference.md). Essentials:

- Engine: Docker (30012→8787), bind-mounted data. `morphod publish`: host-native only, stop container first.
- After export: update ReleaseDatabaseTest count assertions (agent task).
- adb: package `dev.morpho.debug`, uninstall before install, preserve progress via run-as pull/restore.
- Docker build: confirm .dockerignore covers `.claude/` and large directories.
- `MORPHO_API`: unapprove_auto wants `/api` suffix, bulk_approve does not.
