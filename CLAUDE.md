# Morpho Project Instructions

## Cold start

Read `docs/OPERATIONS.md` first (runbook + current state), then `README.md` (architecture whitepaper). Engine runs in Docker on port 30012 (`docker compose up -d`).

## Backlog-driven workflow

All work goes through `docs/BACKLOG.md`. Two modes, never mixed:

### Report mode

When the user reports a problem, request, or idea:
1. Add it to **Inbox** in BACKLOG.md immediately — one line, no evaluation.
2. Acknowledge with the item number. Do NOT start coding, analyzing, or designing.
3. If the user reports multiple items in one message, record all of them.
4. Reporting is instant and never blocked by ongoing work.

### Build mode

When the user says to triage, evaluate, or start implementing:
1. **Triage first**: for each Inbox item, assess effort (S/M/L/XL), priority (P0-P3), dependencies, and feasibility. Move to Ready with estimates, or to Deferred/Won't-do with a clear reason.
2. **Present the plan**: show the ordered Ready list. The user approves, reorders, or removes items before any code is written.
3. **Dispatch to subagents**: write a precise prompt and launch an Agent for each approved item. Independent items run in parallel. The orchestrator (main loop) never writes code directly — it records, evaluates, dispatches, and verifies.
4. **Verify on return**: when a subagent completes, check its output (compilation, tests, correctness). Move to Done with commit reference, or send the agent back with fixes.

### Key rules

- **The orchestrator never codes.** It reads, evaluates, dispatches agents, and verifies results. This keeps the main conversation always responsive to the user.
- Reporting and building are **decoupled**: the user can report items at any time, including while subagents are implementing. Recording an Inbox item never interrupts ongoing work.
- Never start coding from a report. The path is always: Inbox → Triage → Ready → approved → Agent dispatched → verified → Done.
- An item in Ready is a commitment to feasibility, not to immediate implementation. Order matters.
- Deferred is not rejected — it has a reason and can be revisited. Won't-do is permanent with rationale.
- If a subagent mid-implementation discovers the item is larger than estimated, it stops and reports back. The orchestrator re-estimates and consults the user rather than silently expanding scope.

## Code conventions

- Commit style: `type(scope): description` — feat/fix/docs/ops, scope = app/domain/core/admin/ops.
- Working DB is live when the engine runs. Reads: `sqlite3 "file:data/working.db?mode=ro"`. All writes through the admin API only.
- Delete user files only via `gio trash --`. Never `rm`.
- Android: do NOT install to device unless explicitly asked. `adb uninstall dev.morpho` before `adb install` (overlay corrupts the app).
- Release flow: OPERATIONS.md §5-§6 is the runbook. Always preview before export.
- Scorer changes: read OPERATIONS.md §7.8 (the scorer/2→3 incident) before bumping `SCORER_ALGO_VER`.
