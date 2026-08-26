# Wordlist inputs

Production import inputs for morphod. Import order matters: base first, then targets
(`morphod import` never rewrites an existing word's role, so the 1,649 overlap words stay base).

```bash
morphod import --wordlist content/wordlists/base-primary-junior.txt --role base
morphod import --wordlist content/wordlists/target-npee.jsonl --role target
```

| File | Contents | Derivation |
|---|---|---|
| `target-npee.jsonl` | 5,392 NPEE (考研) headwords, 5,303 with IPA, 5,286 with COCA frequency rank | `NPEE_Wordlist.txt` headword+IPA, ranks joined from `COCA_20000.txt` |
| `base-primary-junior.txt` | 1,942 assumed-known base words | union of `小学英语大纲词汇.txt` (463) and `中考英语词汇表.txt` (1,975 lines) headwords |
| `convert_wordlists.py` | the conversion script (stdlib only) | run next to the raw source files |

## Provenance

Raw sources fetched 2026-08-26 from github.com/mahavivo/english-wordlists (master):
`NPEE_Wordlist.txt`, `小学英语大纲词汇.txt`, `中考英语词汇表.txt`, `COCA_20000.txt`.
Wordlists are factual compilations; the NPEE list mirrors the public 考研大纲 vocabulary.
