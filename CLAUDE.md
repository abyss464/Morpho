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
3. **Implement one at a time** (unless items are independent and parallelizable). Move to In Progress, then Done with commit reference.
4. If mid-implementation you discover an item is larger than estimated, stop and re-estimate rather than silently expanding scope.

### Key rules

- Reporting and building are **decoupled**: the user can report items at any time, including while an implementation is running. Recording an Inbox item never interrupts ongoing work.
- Never start coding from a report. The path is always: Inbox → Triage → Ready → approved → In Progress.
- An item in Ready is a commitment to feasibility, not to immediate implementation. Order matters.
- Deferred is not rejected — it has a reason and can be revisited. Won't-do is permanent with rationale.

## Code conventions

- Commit style: `type(scope): description` — feat/fix/docs/ops, scope = app/domain/core/admin/ops.
- Working DB is live when the engine runs. Reads: `sqlite3 "file:data/working.db?mode=ro"`. All writes through the admin API only.
- Delete user files only via `gio trash --`. Never `rm`.
- Android: do NOT install to device unless explicitly asked. `adb uninstall dev.morpho` before `adb install` (overlay corrupts the app).
- Release flow: OPERATIONS.md §5-§6 is the runbook. Always preview before export.
- Scorer changes: read OPERATIONS.md §7.8 (the scorer/2→3 incident) before bumping `SCORER_ALGO_VER`.
