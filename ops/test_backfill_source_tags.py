#!/usr/bin/env python3
"""Fixture tests for ops/backfill_source_tags.py classification (#54).

Builds a tiny working DB and a tiny set of accepted lists exercising every
classification branch, then asserts classify() attributes each candidate to the
right source and flags exactly the genuinely ambiguous one. Read-only against the
fixture DB; writes nothing outside the temp dir.

Run: python3 ops/test_backfill_source_tags.py   (also pytest-discoverable)
"""

import json
import os
import sqlite3
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import backfill_source_tags as bf  # noqa: E402


def _build(tmp):
    logs = os.path.join(tmp, "logs")
    os.makedirs(logs)
    # Accepted lists. cc3m carries word 30 with no DB example (an image whose
    # caption mint failed): it exists only to make word 30 a cc3m word.
    json.dump([{"word_id": 10, "caption": "a cat on a mat"}],
              open(os.path.join(logs, "coco-accepted.json"), "w"))
    json.dump([{"word_id": 20, "caption": "red car parked"}],
              open(os.path.join(logs, "vg-accepted.json"), "w"))
    json.dump([{"word_id": 30, "caption": "sunset over hills"}],
              open(os.path.join(logs, "cc3m-accepted.json"), "w"))

    db = os.path.join(tmp, "fixture.db")
    con = sqlite3.connect(db)
    con.executescript(
        """
        CREATE TABLE image_candidates (
            img_cand_id INTEGER PRIMARY KEY, word_id INTEGER, source TEXT,
            source_ref TEXT, created_at TEXT);
        CREATE TABLE example_candidates (
            ex_cand_id INTEGER PRIMARY KEY, word_id INTEGER, source TEXT,
            source_ref TEXT, created_by TEXT, text TEXT, created_at TEXT);
        """
    )
    images = [
        # (id, word, source, source_ref, created_at)
        (1, 10, "manual", "upload by local", "2026-08-27T17:00:00.000Z"),   # -> coco (pairs ex 1)
        (2, 11, "manual", "upload by local", "2026-08-27T17:05:00.000Z"),   # -> coco (date fallback, no ex)
        (3, 20, "manual", "upload by local", "2026-08-28T11:00:00.000Z"),   # -> vg (pairs ex 2)
        (4, 30, "manual", "upload by local", "2026-08-28T11:10:00.000Z"),   # -> cc3m (word fallback, no ex)
        (5, 60, "openverse", "openverse:1", "2026-08-20T00:00:00.000Z"),    # -> openverse (direct)
        (6, 70, "manual", "upload by operator-genimg", "2026-08-28T14:00:00.000Z"),  # -> codex
    ]
    con.executemany("INSERT INTO image_candidates VALUES (?,?,?,?,?)", images)
    examples = [
        # (id, word, source, source_ref, created_by, text, created_at)
        (1, 10, "manual", None, "admin:local", "a cat on a mat", "2026-08-27T17:00:00.100Z"),  # coco
        (2, 20, "manual", None, "admin:local", "red car parked", "2026-08-28T11:00:00.100Z"),  # vg
        (3, 40, "manual", None, "admin:operator-subs", "he walked home quietly",
         "2026-08-26T09:00:00.000Z"),                                                            # opensubtitles
        (4, 80, "tatoeba", "tatoeba:5", "worker:tatoeba", "a serene lake",
         "2026-08-25T00:00:00.000Z"),                                                            # tatoeba
        (5, 50, "manual", None, "admin:local", "mystery unmatched text",
         "2026-08-28T12:00:00.000Z"),                                                            # FLAGGED
    ]
    con.executemany("INSERT INTO example_candidates VALUES (?,?,?,?,?,?,?)", examples)
    con.commit()
    con.close()
    return db, logs


def run():
    with tempfile.TemporaryDirectory() as tmp:
        db, logs = _build(tmp)
        result = bf.classify(db, logs)

        assert dict(result["image_counts"]) == {
            "coco": 2, "vg": 1, "cc3m": 1, "openverse": 1, "codex": 1
        }, result["image_counts"]
        assert dict(result["example_counts"]) == {
            "coco": 1, "vg": 1, "opensubtitles": 1, "tatoeba": 1
        }, result["example_counts"]

        # Exactly one candidate is genuinely unclassifiable: the manual example
        # that matches no accepted caption and was not subtitle-mined.
        assert len(result["flagged"]) == 1, result["flagged"]
        flag = result["flagged"][0]
        assert (flag["kind"], flag["cand_id"], flag["word_id"]) == ("example", 5, 50), flag

        # Counts sum to the fixture totals (nothing lost, nothing double-counted).
        assert sum(result["image_counts"].values()) == 6
        assert sum(result["example_counts"].values()) == 4

        # A caption-mint-failure image on 2026-08-28 whose word belongs to one
        # #53 dataset is attributed to that dataset, not dropped.
        assert ("cc3m") in dict(result["image_counts"])
        print("OK: all classification branches attribute correctly; 1 flagged as designed.")


# pytest entry point.
def test_classification_branches():
    run()


if __name__ == "__main__":
    run()
