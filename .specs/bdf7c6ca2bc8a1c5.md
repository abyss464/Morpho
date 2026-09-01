---
file: core/crates/reconcile/src/exec/examples.rs
---

# Example Fetch Executor

Gets a word's examples from the exam corpus database, Free Dictionary, or Tatoeba.

## FetchExamplesExecutor

### new(context) → Self
Constructed with the engine context.

### run(job, store) → Result
Selects the source based on the `source` field in the payload:

- **ExamCorpus**: Reads from the locally loaded corpus database at startup, performing no network I/O. If the corpus database is not configured, returns a permanent error.
- **Freedict**: Backfill path — only used for old words that were already in the database when definition fetching had not yet mined examples. New words do not go down this path (the definition executor already commits both outputs in a single pass). A 404 is recorded as an empty result.
- **Tatoeba**: Calls the Tatoeba API to search for examples. A 404 is recorded as an empty result.

All highlight offsets are computed based on the normalized sentence text (handled uniformly by the shared sentence module): they are computed at load time for the corpus database, and at parse time for the two HTTP sources.

### commit(store, word_id, source, examples) → Result
Commits all examples from a single fetch together with the completion mark in one atomic write operation. Used by FetchExamplesExecutor itself, and also called by other modules within the crate (`pub(crate)` visibility).

## Constraints
- Regardless of how many items the source returns (including zero items), the completion mark must be written. Without the mark, the rule will re-derive this task indefinitely.
- Do not run new words through the Freedict backfill path if their examples have already been mined by the definition executor — that only wastes traffic.
