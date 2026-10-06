#!/usr/bin/env python3
"""Replace the example sentence of authored primary lines, by word_id.

  ops/set_examples.py FILE <<< 'word_id | new example sentence'   (one per line)

Every authored file under content/definitions/ is searched; FILE is the edit list.
"""
import glob
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
edits = {}
for line in open(sys.argv[1], encoding="utf-8"):
    if line.strip():
        wid, ex = line.split(" | ", 1)
        edits[wid.strip()] = ex.strip()
for path in sorted(glob.glob(os.path.join(ROOT, "content/definitions/*.txt"))):
    lines = open(path, encoding="utf-8").read().split("\n")
    changed = False
    for i, line in enumerate(lines):
        parts = line.split(" | ")
        if len(parts) == 5 and parts[0] in edits:
            parts[4] = edits.pop(parts[0])
            lines[i] = " | ".join(parts)
            changed = True
    if changed:
        open(path, "w", encoding="utf-8").write("\n".join(lines))
if edits:
    sys.exit(f"not found: {', '.join(edits)}")
