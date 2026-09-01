---
file: core/crates/morphod/src/status.rs
---

# Work Database Status Report

Implementation of the `morphod status` subcommand. Performs a one-shot query of the work database and prints a comprehensive statistical summary.

## StatusReport

Report structure, containing:
- `words` — word count by role (target, base, auxiliary, retired) and ready/blocked status
- `assets` — override rate by asset type (definition, example, image, TTS) (ready/missing/already abandoned) and candidate count
- `oos_open` — number of OOV (out-of-vocabulary) items pending in the queue
- `dead_letters` — number of dead letters (tasks that cannot be processed)
- `plan` — current plan (ID, build time, word count, group count); null when no plan exists
- `events` — total event count
- `releases` — number of exported releases
- `blockers` — histogram of blocking reasons, sorted by occurrence count in descending order

## collect(store, tts) → StatusReport

Reads all statistical data from the work database and assembles it into a report. The TTS override rate is calculated per configured voice using the unified ruling-#13 rule, ensuring consistent counts across the CLI, dashboard, and detail pages. The entire query completes in a single read-only transaction.

## StatusReport::render(db_path, sources, adapters) → string

Renders the report as text, additionally appending:
- External data source status (whether WordNet, corpus databases, etc. are configured)
- Adapter probe results — one row/line per adapter, indicating whether it is usable; when unusable, list the task types that would result in dead letters

## Constraints

- The deduplication and bucketing logic for the TTS override rate must remain consistent with ruling-#13; do not invent counting methods on your own.
- Adapter probing is part of the report (ruling-#17) and must not be omitted — unusable adapters must clearly indicate the affected tasks.
