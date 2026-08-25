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
* `target-words.jsonl` — JSONL with `word`, `phonetic`, `frequency_rank`.

The target set is picked so that the definitions of several entries reference
each other once definitions are fetched — `benevolent` needs `generous`,
`abundant` needs `ample`, `serene` needs `tranquil`, and `lucid` ↔ `coherent`
form a cycle. That gives the wave-2 dependency graph, Tarjan SCC packing and
topological ordering something real to chew on at a size a human can verify by
hand. `adapt` / `adopt` / `adept` are there to exercise distractor binding.
