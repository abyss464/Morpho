"""Hand every automatically-selected slot back to the reconciler.

Approval implies a pin, and a pinned slot is untouchable by automatic selection
(README rule 4). So once `ops/bulk_approve.py` has run, a `scorer_ver` bump
rescores every candidate and moves nothing: the whole library is frozen.

Un-approving an `auto` slot releases the pin approval put there (the engine
keeps a human override's own pin), which lets the next sweep re-select under the
new scorer. Run this after a scorer bump, wait for the engine to converge, then
re-run `ops/bulk_approve.py`.

Usage: python3 ops/unapprove_auto.py [definition|example|image ...]
Defaults to definitions only, which is what a definition-scorer bump touches.
"""

import json
import os
import sys
import urllib.error
import urllib.request
import sqlite3

# The engine listens on 8787 inside its container; compose publishes it on
# 30012. Override with MORPHO_API when running against a native build.
API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012/api")
DB = os.environ.get("MORPHO_DB", os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "data", "working.db"))

KINDS = {
    "definition": (
        "SELECT word_id, pos FROM definition_selections "
        "WHERE approved = 1 AND selected_by = 'auto'",
        lambda row: {"word_id": row[0], "pos": row[1]},
    ),
    "example": (
        "SELECT word_id, slot FROM example_selections "
        "WHERE approved = 1 AND selected_by = 'auto'",
        lambda row: {"word_id": row[0], "slot": row[1]},
    ),
    "image": (
        "SELECT word_id FROM image_selections "
        "WHERE approved = 1 AND selected_by = 'auto'",
        lambda row: {"word_id": row[0]},
    ),
}


def unapprove(kind, body):
    req = urllib.request.Request(
        f"{API}/selections/{kind}/approve",
        data=json.dumps(body).encode(),
        headers={
            "Content-Type": "application/json",
            "X-Morpho-User": "operator-rescore",
        },
        method="DELETE",
    )
    try:
        urllib.request.urlopen(req, timeout=30)
        return True
    except urllib.error.HTTPError:
        return False


def main(kinds):
    conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    for kind in kinds:
        sql, body_of = KINDS[kind]
        ok = failed = 0
        for row in conn.execute(sql).fetchall():
            if unapprove(kind, body_of(row)):
                ok += 1
            else:
                failed += 1
        print(f"{kind}: released {ok}, failed {failed}")


if __name__ == "__main__":
    requested = sys.argv[1:] or ["definition"]
    unknown = [k for k in requested if k not in KINDS]
    if unknown:
        sys.exit(f"unknown slot kind(s): {', '.join(unknown)}")
    main(requested)
