#!/usr/bin/env python3
"""Mine OpenSubtitles for the best example sentences per target word.

Streams the gz corpus once; keeps up to K top-scoring lines per lemma.
Matching is inflection-aware via a form->lemma map built from the lexicon
(same simple detachments the engine's miner accepts: s/es/ies, ed/ied,
ing with e-restoration, doubled-consonant forms).
"""

import gzip, json, re, sys
import sqlite3
from collections import defaultdict

DB = "/home/abysser/Code/learning/Morpho/data/working.db"
CORPUS = "/home/abysser/Code/learning/Morpho/content/corpora/opensubs-en.txt.gz"
OUT = "/tmp/claude-1000/-home-abysser-Code-learning-Morpho/309aaf33-fdae-4549-a568-20411524c24a/scratchpad/mined_sentences.json"
K = 8

conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
lemmas = [r[0].lower() for r in conn.execute(
    "SELECT lemma FROM words WHERE (role='target' OR (role='auxiliary' AND aux_status='active')) AND zh_gloss IS NULL")]

def forms(lemma: str):
    out = {lemma}
    if re.fullmatch(r"[a-z]+", lemma):
        out.add(lemma + "s")
        if lemma.endswith(("s", "x", "z", "ch", "sh")): out.add(lemma + "es")
        if lemma.endswith("y") and len(lemma) > 2 and lemma[-2] not in "aeiou":
            out.update({lemma[:-1] + "ies", lemma[:-1] + "ied"})
        if lemma.endswith("e"):
            out.update({lemma + "d", lemma[:-1] + "ing"})
        else:
            out.update({lemma + "ed", lemma + "ing"})
            if len(lemma) > 2 and lemma[-1] not in "aeiouwxy" and lemma[-2] in "aeiou" and lemma[-3] not in "aeiou":
                out.update({lemma + lemma[-1] + "ed", lemma + lemma[-1] + "ing"})
    return out

form2lemma = {}
for lm in lemmas:
    for f in forms(lm):
        form2lemma.setdefault(f, lm)

word_re = re.compile(r"[a-z']+")
bad_re = re.compile(r"\d|[#@_*\[\]{}|\\/<>]|www\.|http")
name_re = re.compile(r"\b[A-Z][a-z]+ [A-Z][a-z]+\b")

best: dict = defaultdict(list)   # lemma -> [(score, sentence)]

def score(line: str, wc: int) -> float:
    s = 0.0
    if 7 <= wc <= 13: s += 3
    elif 6 <= wc <= 16: s += 1.5
    if line[0].isupper(): s += 1
    if line.endswith("."): s += 1.5
    elif line.endswith("!"): s += 0.7
    if '"' in line or "..." in line or line.endswith("?"): s -= 2
    if line.count(",") > 2: s -= 1
    if name_re.search(line): s -= 0.8
    if line.isupper(): s -= 3
    return s

seen_hash = set()
n = 0
with gzip.open(CORPUS, "rt", encoding="utf-8", errors="ignore") as fh:
    for line in fh:
        n += 1
        line = line.strip()
        if not (20 <= len(line) <= 110) or bad_re.search(line):
            continue
        low = line.lower()
        toks = word_re.findall(low)
        wc = len(toks)
        if wc < 6 or wc > 16:
            continue
        hit = None
        for t in toks:
            lm = form2lemma.get(t)
            if lm is not None:
                hit = lm
                break
        if hit is None:
            continue
        lst = best[hit]
        if len(lst) >= K and lst[-1][0] >= 6.5:
            continue
        h = hash(low)
        if h in seen_hash:
            continue
        sc = score(line, wc)
        if sc <= 0:
            continue
        seen_hash.add(h)
        lst.append((sc, line))
        lst.sort(key=lambda x: -x[0])
        del lst[K:]
        if n % 20_000_000 == 0:
            print(f"{n} lines, covered {len(best)}/{len(lemmas)}", flush=True)

print(f"done: {n} lines, covered {len(best)}/{len(lemmas)}", flush=True)
json.dump({k: v for k, v in best.items()}, open(OUT, "w"))
