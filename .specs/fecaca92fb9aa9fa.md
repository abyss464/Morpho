---
file: core/crates/reconcile/src/stages/select.rs
---

# Scoring and automatic selection stage

1. Candidates whose `scorer_ver` is behind get rescored.
2. Empty slots take the highest-scoring available candidate.
3. Auto, unpinned slots switch only when a challenger clears the hysteresis margin.
4. Pinned slots are never touched.
5. The first sense a word gets is marked primary by strongest frequency evidence.
6. A reconciler-picked primary moves when evidence does; an editor-placed primary is never moved.

Example slots use a shared candidate pool across all three slots; selection assigns candidates to slots without duplication.
