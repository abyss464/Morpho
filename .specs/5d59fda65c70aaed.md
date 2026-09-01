---
file: core/crates/reconcile/src/sources/tatoeba.rs
---

# Tatoeba sentence search

Client for `https://tatoeba.org/en/api_v0/search`. Community corpus of natural sentences, free of credentials. Records per-sentence licence (CC BY 2.0 FR, CC0 1.0, etc.) on each candidate. Results are relevance-ranked, so every sentence is re-checked locally against the lemma; sentences that do not actually contain the word (or a simple inflection) are dropped.
