# Dev fixtures

Tiny word lists for local development and tests. They are *not* the real
syllabus data — the production lists are ~2000 base words and ~5500 target
words, and they arrive through the same `morphod import` path.

```
morphod import --wordlist core/fixtures/base-words.txt   --role base
morphod import --wordlist core/fixtures/target-words.jsonl --role target
```

* `base-words.txt` — plain text, one word per line. Deliberately includes
  function words (`and`, `of`, `to`, …): the tokenizer emits every token, so
  anything absent from the base list surfaces in the OOV queue.
* `target-words.jsonl` — JSONL with `word`, and optionally `phonetic` and
  `frequency_rank`.

60 target words. The first 25 carry a phonetic and a frequency rank; the other
35 carry only the lemma, because those two fields are real syllabus data we do
not have for them and inventing them would poison the ordering. A word with no
rank sorts last, and the Free Dictionary fills the phonetic in on the first
fetch — a gap the engine closes with real data rather than a placeholder.

The set is picked so that the definitions of several entries reference each
other once definitions are fetched — `benevolent` needs `generous`, `abundant`
needs `ample`, `serene` needs `tranquil`, and `lucid` ↔ `coherent` form a
cycle. That gives the dependency graph, Tarjan SCC packing and topological
ordering something real to chew on at a size a human can verify by hand.
`adapt` / `adopt` / `adept` are there to exercise distractor binding.

## What a live run converges

With no image credentials, no `wordnet_dir` and no `corpus_path`, a run against
the real network converges to: real Free Dictionary definitions, automatic
selection, extraction, the OOV queue, the dependency graph, the plan, real
edge-tts audio — and every word blocked on `missing_image` and
`missing_example`. That is the correct end state, and `morphod export
--preview` says so word by word.
