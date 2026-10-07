#!/usr/bin/env python3
"""Backfill the #54 `source` provenance tag onto every existing candidate.

Item #53 ingested VG / CC3M / WIT / Wikimedia-Commons image+caption pairs through
the manual upload endpoints, all tagged `source = 'manual'` — indistinguishable
from COCO and from the operator's Codex generations, which are also `manual`. This
script reconstructs the true provenance of every image and example candidate and
assigns it as the required `source` tag (`candidate_tag`, category `source`),
without touching the `source` column (so no selection can move).

Classification (each candidate resolves to exactly one source):

  Images (image_candidates):
    * stock/generated columns map directly: openverse, wikimedia, pixabay,
      sdxl (and unsplash/pexels/codex if ever present as a column value);
    * source='manual' with source_ref 'upload by operator-genimg' /
      'upload by operator-regen*' / 'upload by ops-genimg2w'  -> codex;
    * source='manual', source_ref='upload by local' -> COCO or a #53 dataset,
      resolved by pairing the image to the caption-matched example of the same
      word created within seconds of it (the manual ingests uploaded the image
      and minted the caption back to back). An image with no pairable example
      (a caption-mint failure, or the one pre-batch COCO upload) falls back to
      created_at date (2026-08-27 -> coco) and, on 2026-08-28, to the single
      #53 dataset its word belongs to.

  Examples (example_candidates):
    * tatoeba / freedict / exam_corpus / llm columns map directly;
    * source='manual' whose (word_id, canonical text) matches an accepted list
      -> that dataset (coco/vg/cc3m/wit/commons);
    * source='manual', created_by 'admin:operator-subs' -> opensubtitles
      (the ops/mine_subs.py sentences, release 1.4);
    * anything else -> FLAGGED (an unknown; the dangerous case).

'vg' folds the VG batches, 'cc3m' folds the CC3M literal + inflected runs.

Usage:
  backfill_source_tags.py dry-run [--db PATH] [--logs DIR]
      Read-only. Reads the working DB with `mode=ro` and the ops logs, prints
      the per-source reconciliation table and any unclassified candidates, and
      writes NOTHING.
  backfill_source_tags.py apply  [--db PATH] [--logs DIR] [--api URL] [--user U]
      Assigns each candidate's source tag through the admin API
      (POST /candidates/{kind}/{cand_id}/tags). Idempotent and resumable — a tag
      already at the right value is a no-op. Refuses to run while any candidate
      is unclassified. This is the post-merge/redeploy ops step; it is never run
      from the development worktree.
"""

import argparse
import datetime
import json
import os
import sqlite3
import sys
import unicodedata
import urllib.request
from collections import Counter, defaultdict

ROOT = os.environ.get("MORPHO_ROOT", os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEFAULT_DB = f"{ROOT}/data/working.db"
DEFAULT_LOGS = f"{ROOT}/ops/logs"

# Accepted-list files per dataset. vg2 folds into vg; cc3m-inflected into cc3m.
DATASETS = {
    "coco": ["coco-accepted.json"],
    "vg": ["vg-accepted.json", "vg2-accepted.json"],
    "cc3m": ["cc3m-accepted.json", "cc3m-inflected-accepted.json"],
    "wit": ["wit-accepted.json"],
    "commons": ["commons-accepted.json"],
}
# Priority when a (word_id, caption) somehow appears in more than one list.
PRIORITY = ["coco", "vg", "cc3m", "wit", "commons"]

CODEX_REFS = (
    "upload by operator-genimg",
    "upload by operator-regen",
    "upload by ops-genimg2w",
)
LOCAL_REF = "upload by local"
COCO_LAST_DAY = "2026-08-27"  # the manual-local ingest before #53 was COCO's
PAIR_WINDOW_S = 60.0          # image and its caption were minted back to back


def canon(text):
    """Replicate morphod's canonicalize: NFC, trim, collapse internal whitespace."""
    text = unicodedata.normalize("NFC", text)
    out = []
    pending = False
    for ch in text:
        if ch.isspace() or unicodedata.category(ch) == "Zs":
            if out:
                pending = True
        else:
            if pending:
                out.append(" ")
                pending = False
            out.append(ch)
    return "".join(out)


def _secs(iso):
    return datetime.datetime.fromisoformat(iso.replace("Z", "+00:00")).timestamp()


def load_accepted(logs_dir):
    """Return (example_key, word_datasets): (word_id, canon_caption)->dataset, and
    word_id->set(datasets); plus per-file lengths for the report."""
    example_key = {}
    word_datasets = defaultdict(set)
    lengths = {}
    for dataset in PRIORITY:
        for fname in DATASETS[dataset]:
            path = os.path.join(logs_dir, fname)
            if not os.path.exists(path):
                lengths[fname] = None
                continue
            entries = json.load(open(path))
            lengths[fname] = len(entries)
            for e in entries:
                key = (e["word_id"], canon(e["caption"]))
                example_key.setdefault(key, dataset)
                word_datasets[e["word_id"]].add(dataset)
    return example_key, word_datasets, lengths


def classify(db_path, logs_dir):
    """Classify every image and example candidate. Returns a dict with
    per-candidate assignments, per-source counts, flagged unknowns, and the
    accepted-list lengths. Read-only: opens the DB with mode=ro."""
    example_key, word_datasets, lengths = load_accepted(logs_dir)
    con = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)

    image_assign = []   # (cand_id, source)
    example_assign = []
    flagged = []

    # Examples first — their caption match also drives image pairing.
    ex_by_word = defaultdict(list)  # word_id -> [(created_at_secs, dataset)]
    for cid, wid, source, sref, cby, text, created in con.execute(
        "SELECT ex_cand_id, word_id, source, source_ref, created_by, text, created_at "
        "FROM example_candidates"
    ):
        if source != "manual":
            example_assign.append((cid, source))
            continue
        dataset = example_key.get((wid, text))
        if dataset:
            example_assign.append((cid, dataset))
            ex_by_word[wid].append((_secs(created), dataset))
        elif cby == "admin:operator-subs":
            example_assign.append((cid, "opensubtitles"))
        else:
            flagged.append(
                {"kind": "example", "cand_id": cid, "word_id": wid,
                 "source_ref": sref, "created_by": cby, "text": text[:80]}
            )

    # Images.
    for cid, wid, source, sref, created in con.execute(
        "SELECT img_cand_id, word_id, source, source_ref, created_at FROM image_candidates"
    ):
        if source != "manual":
            image_assign.append((cid, source))
            continue
        ref = sref or ""
        if ref.startswith(CODEX_REFS):
            image_assign.append((cid, "codex"))
            continue
        if ref == LOCAL_REF:
            dataset = _pair_image(wid, created, ex_by_word, word_datasets)
            if dataset:
                image_assign.append((cid, dataset))
            else:
                flagged.append(
                    {"kind": "image", "cand_id": cid, "word_id": wid,
                     "source_ref": ref, "created_at": created,
                     "word_datasets": sorted(word_datasets.get(wid, set()))}
                )
            continue
        flagged.append(
            {"kind": "image", "cand_id": cid, "word_id": wid,
             "source_ref": ref, "created_at": created, "word_datasets": []}
        )

    con.close()
    return {
        "image_assign": image_assign,
        "example_assign": example_assign,
        "image_counts": Counter(s for _, s in image_assign),
        "example_counts": Counter(s for _, s in example_assign),
        "flagged": flagged,
        "lengths": lengths,
    }


def _pair_image(word_id, created_at, ex_by_word, word_datasets):
    """Attribute one manual-local image to a dataset."""
    t = _secs(created_at)
    cands = ex_by_word.get(word_id)
    if cands:
        best = min(cands, key=lambda c: abs(c[0] - t))
        if abs(best[0] - t) <= PAIR_WINDOW_S:
            return best[1]  # the caption minted alongside this image
    # No caption paired within the window: fall back.
    if created_at[:10] <= COCO_LAST_DAY:
        return "coco"
    others = word_datasets.get(word_id, set()) - {"coco"}
    if len(others) == 1:
        return next(iter(others))
    return None  # ambiguous -> flag


# ---------------------------------------------------------------------------
# dry-run report
# ---------------------------------------------------------------------------

def cmd_dry_run(args):
    result = classify(args.db, args.logs)
    img, ex = result["image_counts"], result["example_counts"]
    img_total, ex_total = sum(img.values()), sum(ex.values())

    print("=== accepted lists ===")
    for fname, n in result["lengths"].items():
        print(f"  {fname:34} {'MISSING' if n is None else n}")

    def table(title, counts, total, db_total):
        print(f"\n=== {title} by source ({total} classified) ===")
        for src in sorted(counts, key=lambda s: -counts[s]):
            print(f"  {src:16} {counts[src]}")
        flag = " <-- MISMATCH" if total != db_total else ""
        print(f"  {'sum':16} {total}   (db total {db_total}){flag}")

    con = sqlite3.connect(f"file:{args.db}?mode=ro", uri=True)
    db_img = con.execute("SELECT COUNT(*) FROM image_candidates").fetchone()[0]
    db_ex = con.execute("SELECT COUNT(*) FROM example_candidates").fetchone()[0]
    con.close()

    table("IMAGE candidates", img, img_total, db_img)
    table("EXAMPLE candidates", ex, ex_total, db_ex)

    flagged = result["flagged"]
    print(f"\n=== UNCLASSIFIED (dangerous) candidates: {len(flagged)} ===")
    for row in flagged[:50]:
        print(f"  {row}")
    if len(flagged) > 50:
        print(f"  ... and {len(flagged) - 50} more")

    ok = img_total == db_img and ex_total == db_ex and not flagged
    verdict = (
        "CLEAN — every candidate classified, counts reconcile"
        if ok
        else "INCOMPLETE — see mismatches / unclassified above"
    )
    print(f"\nRESULT: {verdict}")
    return 0 if ok else 1


# ---------------------------------------------------------------------------
# apply (post-merge ops step; never run from the worktree)
# ---------------------------------------------------------------------------

def _post(api, user, path, body):
    req = urllib.request.Request(
        f"{api}{path}",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "X-Morpho-User": user},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        return resp.status


def cmd_apply(args):
    result = classify(args.db, args.logs)
    if result["flagged"]:
        print(f"REFUSING: {len(result['flagged'])} candidate(s) are unclassified; "
              "resolve them before writing tags. Run dry-run for the list.",
              file=sys.stderr)
        return 1

    plan = [("image", cid, src) for cid, src in result["image_assign"]]
    plan += [("example", cid, src) for cid, src in result["example_assign"]]
    print(f"assigning source tags to {len(plan)} candidates via {args.api} ...")
    done = 0
    for kind, cid, src in plan:
        _post(args.api, args.user, f"/candidates/{kind}/{cid}/tags",
              {"category": "source", "value": src})
        done += 1
        if done % 2000 == 0:
            print(f"  ...{done}/{len(plan)}")
    print(f"DONE: {done} source tags assigned.")
    return 0


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest="mode", required=True)
    for name, func in (("dry-run", cmd_dry_run), ("apply", cmd_apply)):
        sp = sub.add_parser(name)
        sp.add_argument("--db", default=DEFAULT_DB)
        sp.add_argument("--logs", default=DEFAULT_LOGS)
        sp.add_argument("--api", default=os.environ.get("MORPHO_API", "http://127.0.0.1:30012/api"))
        sp.add_argument("--user", default="backfill-source")
        sp.set_defaults(func=func)
    args = p.parse_args()
    sys.exit(args.func(args))


if __name__ == "__main__":
    main()
