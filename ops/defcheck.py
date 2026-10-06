#!/usr/bin/env python3
"""Vocabulary check for authored definitions, mirroring the engine's extraction.

Tokenizes and lemmatizes each definition the way `reconcile::text` and
`reconcile::morphy` do (simple tokenizer, Morphy over the `words` lexicon with
the WNdb exception lists layered on the compiled-in table), then classifies
every lemma:

  self   the headword itself (the dependency view drops it)
  base   a base word: no dependency, always readable
  dep    a target/auxiliary word that ships in the latest release: readable,
         adds a dependency edge
  gloss  a word anchored by a Chinese gloss: rejected, definitions stay English
  unship a lexicon word that does not ship (retired auxiliary, blocked target)
  oov    not in the lexicon at all

Input: lines `word_id | pos | primary | definition [| example]` (the format of
content/definitions/*.txt): pos is n/v/adj/adv/prep/conj/interj/phrase,
primary is `*` or `-`, and the example sentence sits on the primary line only.
Lines starting with `#` are ignored. Every example must contain the headword
(any regular inflection); a line whose example does not is rejected.

  ops/defcheck.py FILE...           report rejected tokens, exit 1 if any
  ops/defcheck.py --deps FILE...    also list dependency tokens per line
"""

import argparse
import glob
import os
import sqlite3
import sys
import unicodedata

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKING_DB = os.environ.get("MORPHO_DB", os.path.join(ROOT, "data/working.db"))
WORDNET_DIR = os.environ.get("MORPHO_WORDNET_DIR", os.path.join(ROOT, "data/wordnet/dict"))
BUILTIN_EXC = os.path.join(ROOT, "core/crates/reconcile/data/irregular.exc")

ABBREVIATIONS = ["e.g.", "i.e.", "etc.", "vs.", "e.g", "i.e", "etc", "sth", "vs", "sb"]
DETACH_RULES = [
    ("ches", "ch"), ("shes", "sh"), ("iest", "y"), ("ses", "s"), ("xes", "x"),
    ("zes", "z"), ("men", "man"), ("ies", "y"), ("oes", "o"), ("ing", "e"),
    ("ing", ""), ("est", "e"), ("est", ""), ("ier", "y"), ("es", "e"),
    ("ed", "e"), ("ed", ""), ("er", "e"), ("er", ""), ("s", ""),
]
DOUBLING_SUFFIXES = ["ing", "ed"]
DOUBLING_CONSONANTS = set("bdglmnprtvz")
MIN_CANDIDATE_CHARS = 3
APOSTROPHES = ("'", "’")
POS = {"n": "noun", "v": "verb", "adj": "adj", "adv": "adv", "prep": "prep", "conj": "conj", "interj": "interj", "phrase": "phrase"}


LOCATE_SUFFIXES = ["ies", "ing", "ied", "ees", "es", "ed", "en", "er", "s", "d"]


def locate(text, lemma):
    """The engine's `sentence::locate`: an example is refused unless this finds the word."""
    lower, needle = text.lower(), lemma.lower()
    if not needle:
        return None
    forms = [needle]
    for suffix in LOCATE_SUFFIXES:
        forms.append(needle + suffix)
        if needle.endswith("e"):
            forms.append(needle[:-1] + suffix)
    is_word = lambda c: c.isalnum() or c in APOSTROPHES
    for form in forms:
        start = lower.find(form)
        while start != -1:
            end = start + len(form)
            if (start == 0 or not is_word(lower[start - 1])) and (end == len(lower) or not is_word(lower[end])):
                return start, end
            start = lower.find(form, start + 1)
    return None


def latest_release_db():
    dirs = sorted(glob.glob(os.path.join(ROOT, "data/releases/export-*/release.db")))
    if not dirs:
        sys.exit("no release found under data/releases/")
    return dirs[-1]


def fold(s):
    return unicodedata.normalize("NFC", s).strip().lower()


def tokenize(text):
    chars = list(text)
    n = len(chars)
    tokens = []
    i = 0
    while i < n:
        if not chars[i].isalpha():
            i += 1
            continue
        skip = abbreviation_at(chars, i)
        if skip:
            i += skip
            continue
        start = i
        while i < n:
            if chars[i].isalpha():
                i += 1
                continue
            if chars[i] in APOSTROPHES and i > start and i + 1 < n and chars[i + 1].isalpha():
                i += 1
                continue
            break
        raw = "".join("'" if c in APOSTROPHES else c for c in chars[start:i])
        if raw.endswith("'s") and len(raw) > 2:
            raw = raw[:-2]
        if raw:
            tokens.append(raw)
    return tokens


def abbreviation_at(chars, start):
    for abbr in ABBREVIATIONS:
        end = start + len(abbr)
        if end > len(chars):
            continue
        if "".join(chars[start:end]).lower() != abbr:
            continue
        nxt = chars[end] if end < len(chars) else ""
        if nxt and (nxt.isalpha() or nxt in APOSTROPHES):
            continue
        return len(abbr)
    return 0


def load_exceptions():
    def absorb(path, table):
        loaded = False
        try:
            with open(path, encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    fields = line.split()
                    surface = fields[0]
                    if "_" in surface or "-" in surface:
                        continue
                    slot = table.setdefault(surface, [])
                    for lemma in fields[1:]:
                        if "_" in lemma or "-" in lemma:
                            continue
                        lemma = fold(lemma)
                        if lemma and lemma not in slot:
                            slot.append(lemma)
                    loaded = True
        except OSError:
            pass
        return loaded

    table = {}
    absorb(BUILTIN_EXC, table)
    wndb = {}
    any_loaded = False
    for name in ("noun.exc", "verb.exc", "adj.exc", "adv.exc"):
        any_loaded |= absorb(os.path.join(WORDNET_DIR, name), wndb)
    if any_loaded:
        table.update(wndb)
    return table


def is_vowel(c):
    return c in "aeiou"


def undouble(folded):
    out = []
    for suffix in DOUBLING_SUFFIXES:
        if not folded.endswith(suffix):
            continue
        stem = folded[: -len(suffix)]
        if len(stem) < 2:
            continue
        last, prev = stem[-1], stem[-2]
        if last == prev and last in DOUBLING_CONSONANTS:
            cand = stem[:-1]
            if len(cand) >= MIN_CANDIDATE_CHARS:
                out.append(cand)
    return out


class Lemmatizer:
    def __init__(self, lexicon, exceptions):
        self.lexicon = lexicon
        self.exceptions = exceptions

    def __call__(self, surface):
        folded = fold(surface)
        if not folded:
            return folded
        if folded in self.lexicon:
            return folded
        for cand in self.exceptions.get(folded, []):
            if cand == folded:
                return folded
            if cand in self.lexicon:
                return cand
        for suffix, repl in DETACH_RULES:
            if folded.endswith(suffix):
                stem = folded[: -len(suffix)]
                if not stem:
                    continue
                cand = stem + repl
                if len(cand) >= MIN_CANDIDATE_CHARS and cand in self.lexicon:
                    return cand
        for cand in undouble(folded):
            if cand in self.lexicon:
                return cand
        return folded


class Checker:
    def __init__(self):
        wconn = sqlite3.connect(f"file:{WORKING_DB}?mode=ro", uri=True)
        self.words = {}
        self.by_id = {}
        for wid, lemma, role, gloss in wconn.execute("SELECT word_id, lemma, role, zh_gloss FROM words"):
            self.words[fold(lemma)] = (wid, role, gloss)
            self.by_id[wid] = lemma
        rconn = sqlite3.connect(f"file:{latest_release_db()}?mode=ro", uri=True)
        self.shipping = {wid for (wid,) in rconn.execute("SELECT word_id FROM words")}
        self.lemmatize = Lemmatizer(set(self.words), load_exceptions())

    def example_has_head(self, word_id, example):
        return locate(example, self.by_id.get(word_id, "")) is not None

    def classify(self, word_id, text):
        head = fold(self.by_id.get(word_id, ""))
        out = []
        for surface in tokenize(text):
            lemma = self.lemmatize(surface)
            entry = self.words.get(lemma)
            if lemma == head:
                kind = "self"
            elif entry is None:
                kind = "oov"
            else:
                wid, role, gloss = entry
                if gloss:
                    kind = "gloss"
                elif role == "base":
                    kind = "base"
                elif wid in self.shipping:
                    kind = "dep"
                else:
                    kind = "unship"
            out.append((surface, lemma, kind))
        return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("files", nargs="+")
    ap.add_argument("--deps", action="store_true", help="list dependency tokens per line")
    args = ap.parse_args()

    checker = Checker()
    bad_lines = 0
    total = 0
    dep_lines = 0
    no_self = 0
    for path in args.files:
        with open(path, encoding="utf-8") as f:
            for lineno, line in enumerate(f, 1):
                line = line.rstrip("\n")
                if not line.strip() or line.startswith("#"):
                    continue
                parts = [p.strip() for p in line.split(" | ")]
                if len(parts) not in (4, 5) or not parts[0].isdigit() or parts[1] not in POS or parts[2] not in ("*", "-"):
                    print(f"{path}:{lineno}: malformed line")
                    bad_lines += 1
                    continue
                wid, pos, primary, text = int(parts[0]), parts[1], parts[2], parts[3]
                example = parts[4] if len(parts) == 5 else ""
                if (primary == "*") != bool(example):
                    print(f"{path}:{lineno}: the primary line, and only it, carries the example")
                    bad_lines += 1
                    continue
                total += 1
                head = checker.by_id.get(wid, f"#{wid}")
                tokens = checker.classify(wid, text)
                bad = [f"{s}->{l}:{k}" for s, l, k in tokens if k in ("oov", "gloss", "unship")]
                deps = sorted({l for _, l, k in tokens if k == "dep"})
                if not any(k == "self" for _, _, k in tokens):
                    no_self += 1
                if deps:
                    dep_lines += 1
                if example and not checker.example_has_head(wid, example):
                    bad.append("example-lacks-headword")
                if bad:
                    bad_lines += 1
                    print(f"{path}:{lineno}: {head} [{pos}] REJECT {' '.join(bad)}")
                elif args.deps and deps:
                    print(f"{path}:{lineno}: {head} [{pos}] deps {' '.join(deps)}")
    print(f"checked {total} definitions: {bad_lines} rejected, {dep_lines} with dependencies, {no_self} without the headword")
    sys.exit(1 if bad_lines else 0)


if __name__ == "__main__":
    main()
