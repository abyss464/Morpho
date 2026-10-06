#!/usr/bin/env python3
"""Reopen the image slots of words whose pools gained photos after approval.

An approved (or pinned) image selection is frozen: the engine's selector will
not replace it, however good a picture uploaded later is. For every word that
ops/expand_images.py uploaded to, this re-selects the current picture unpinned
and withdraws its approval, so the engine re-ranks the widened pool under its
own rules (CLIP aptness, the quality prior, distinct pictures within a
question) before the next release approves it again.

  ops/reselect_images.py [--log ops/logs/expand_images.jsonl] [--dry-run]
"""

import argparse
import json
import os
import sqlite3
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mint_authored import api, release_image  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DB = os.path.join(ROOT, "data/working.db")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--log", default=os.path.join(ROOT, "ops/logs/expand_images.jsonl"))
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    words = set()
    for line in open(args.log):
        record = json.loads(line)
        if any(u.get("status") in ("200", "201") for u in record["uploads"]):
            words.add(record["word_id"])
    conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    errors = 0
    for wid in sorted(words):
        def call(method, path, body):
            nonlocal errors
            if args.dry_run:
                return True
            status, err = api(method, path, body)
            if status not in (200, 201):
                errors += 1
                print(f"{wid}: {method} {path} -> {status} {err}")
                return False
            return True
        release_image(conn, wid, call)
    print(f"{len(words)} words reopened, {errors} errors")


if __name__ == "__main__":
    main()
