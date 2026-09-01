---
file: core/crates/reconcile/src/sources/sentence.rs
---

Shared sentence processing for all example sources. Canonicalizes text first (collapsing whitespace), then locates the target word case-insensitively with whole-word matching including regular English inflections. Sentences where the target cannot be found are dropped -- a wrong highlight is worse than a missing example.
