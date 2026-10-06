#!/usr/bin/env python3
"""Print the writing brief for authored definitions, one word per line.

For every word in the latest release (targets first, then auxiliaries, each in
learning order) prints its enabled definition slots with their current text and
its slot-1 and slot-2 example sentences as reference material. The authored
file replaces slot 1 with its own example, written for the primary sense, and
the picture is then matched to that example.

  ops/def_inputs.py --offset 0 --limit 150 [--role target|auxiliary] [--skip FILE...]

`--skip` drops words that already have a line in the given authored files.
Line format:  word_id word /phonetic/ || pos*: current text || ... || 1: sentence || 2: sentence
(`*` marks the current primary slot).
"""

import argparse
import glob
import os
import sqlite3

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKING_DB = os.environ.get("MORPHO_DB", os.path.join(ROOT, "data/working.db"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--offset", type=int, default=0)
    ap.add_argument("--limit", type=int, default=150)
    ap.add_argument("--role", choices=["target", "auxiliary"])
    ap.add_argument("--skip", nargs="*", default=[])
    args = ap.parse_args()

    release = sorted(glob.glob(os.path.join(ROOT, "data/releases/export-*/release.db")))[-1]
    r = sqlite3.connect(f"file:{release}?mode=ro", uri=True)
    w = sqlite3.connect(f"file:{WORKING_DB}?mode=ro", uri=True)

    done = set()
    for path in args.skip:
        with open(path, encoding="utf-8") as f:
            for line in f:
                head = line.split(" | ", 1)[0].strip()
                if head.isdigit():
                    done.add(int(head))

    where = "WHERE role = ?" if args.role else ""
    params = (args.role,) if args.role else ()
    rows = r.execute(
        f"SELECT word_id, word, phonetic, role FROM words {where} "
        "ORDER BY role = 'auxiliary', learning_order",
        params,
    ).fetchall()
    rows = [row for row in rows if row[0] not in done][args.offset : args.offset + args.limit]

    for wid, word, phonetic, _role in rows:
        slots = w.execute(
            "SELECT ds.pos, ds.is_primary, dc.text FROM definition_selections ds "
            "JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id "
            "WHERE ds.word_id = ? AND ds.enabled = 1 ORDER BY ds.is_primary DESC, ds.pos",
            (wid,),
        ).fetchall()
        exs = r.execute(
            "SELECT display_order, sentence FROM examples WHERE word_id = ? AND display_order <= 2 "
            "ORDER BY display_order",
            (wid,),
        ).fetchall()
        parts = [f"{wid} {word} {phonetic or ''}".rstrip()]
        parts += [f"{pos}{'*' if prim else ''}: {text}" for pos, prim, text in slots]
        parts += [f"{n}: {s if len(s) <= 140 else s[:137] + '...'}" for n, s in exs]
        print(" || ".join(parts))


if __name__ == "__main__":
    main()
