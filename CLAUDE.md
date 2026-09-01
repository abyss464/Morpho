# Morpho Project Instructions

## Cold start

Read `docs/OPERATIONS.md` first (runbook + current state), then `README.md` (architecture whitepaper). Engine runs in Docker on port 30012 (`docker compose up -d`).

## Code conventions

- Commit style: `type(scope): description`, type = feat/fix/docs/ops, scope = app/domain/core/admin/ops
- Working DB read-only: `sqlite3 "file:data/working.db?mode=ro"`, writes via admin API only
- Release flow: OPERATIONS.md §5-§6, preview before export
