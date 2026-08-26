#!/usr/bin/env python3
"""Convert fetched wordlist sources into Morpho import formats.

Inputs (same directory):
  npee.txt     - NPEE (kaoyan) list: "word [IPA] pos.gloss..."
  primary.txt  - primary-school words, one per line
  zhongkao.txt - junior-high list: "word (variant) [IPA] pos. gloss" (BOM, wide chars)
  coca.txt     - COCA 20000, frequency-ordered, one word per line

Outputs:
  target-npee.jsonl            {"word","phonetic","frequency_rank"?}
  base-primary-junior.txt      merged deduped base list, one word per line
"""

import json
import re
import unicodedata
from pathlib import Path

HERE = Path(__file__).parent
WORD_RE = re.compile(r"^[A-Za-z][A-Za-z'.-]*$")
IPA_RE = re.compile(r"\[([^\]]+)\]")


def clean(tok: str) -> str:
    tok = unicodedata.normalize("NFC", tok.strip().lstrip("﻿"))
    return tok.strip(",;")


def coca_ranks() -> dict[str, int]:
    ranks: dict[str, int] = {}
    for i, line in enumerate((HERE / "coca.txt").read_text().splitlines(), 1):
        w = clean(line).lower()
        if w and w not in ranks:
            ranks[w] = i
    return ranks


def parse_headword(line: str) -> list[str]:
    """First token(s); 'a (an)' yields both forms."""
    line = line.lstrip("﻿").strip()
    if not line:
        return []
    head = line.split("[")[0]
    head = head.replace("（", "(").replace("）", ")")
    out = []
    m = re.match(r"^([A-Za-z][A-Za-z'.-]*)\s*(?:\(([A-Za-z][A-Za-z'.-]*)\))?", head)
    if m:
        out.append(m.group(1))
        if m.group(2):
            out.append(m.group(2))
    return [w for w in (clean(x) for x in out) if WORD_RE.match(w)]


def main() -> None:
    ranks = coca_ranks()

    base: dict[str, None] = {}
    for src in ("primary.txt", "zhongkao.txt"):
        for line in (HERE / src).read_text(encoding="utf-8-sig").splitlines():
            for w in parse_headword(line):
                base.setdefault(w.lower(), None)

    targets: dict[str, dict] = {}
    for line in (HERE / "npee.txt").read_text().splitlines():
        words = parse_headword(line)
        if not words:
            continue
        w = words[0]
        key = w.lower()
        if key in targets:
            continue
        ipa = IPA_RE.search(line)
        entry: dict = {"word": w}
        if ipa:
            entry["phonetic"] = "/" + ipa.group(1).strip() + "/"
        if key in ranks:
            entry["frequency_rank"] = ranks[key]
        targets[key] = entry

    out_t = HERE / "target-npee.jsonl"
    with out_t.open("w") as f:
        for e in targets.values():
            f.write(json.dumps(e, ensure_ascii=False) + "\n")

    out_b = HERE / "base-primary-junior.txt"
    out_b.write_text("\n".join(sorted(base)) + "\n")

    overlap = sum(1 for k in targets if k in base)
    with_rank = sum(1 for e in targets.values() if "frequency_rank" in e)
    with_ipa = sum(1 for e in targets.values() if "phonetic" in e)
    print(f"targets: {len(targets)} (ipa {with_ipa}, rank {with_rank})")
    print(f"base: {len(base)}; target∩base: {overlap} (import order makes these stay base)")


if __name__ == "__main__":
    main()
