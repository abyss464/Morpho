#!/usr/bin/env python3
"""Write the credits for third-party text shipped in the app's release.db (NOTICE.md).

Every example sentence in the release is traced back to its candidate in the working
database, and every etymology to its source, and listed with where it came from and under
which licence:

  content/attribution/examples.tsv   word_id, word, sentence, source, reference, licence
  content/attribution/etymology.tsv  word_id, word, source, reference, licence
  content/attribution/images.tsv     word_id, word, file, source, reference, author, licence

Pictures are not in the repository, but the APK carries them. Their credits come from the
engine where it fetched the picture itself, from ops/logs/expand_images.jsonl for pictures
the widening script uploaded, and for COCO pictures from content/attribution/coco.tsv (built
from ops/logs/coco-accepted.json and the COCO 2014 caption annotations passed with --coco).

ops/release.sh runs it after copying a new release.db. Read-only on both databases.

    python3 ops/attribution.py [--release PATH] [--db PATH] [--out DIR] [--coco FILE ...]
"""

import argparse
import csv
import json
import os
import sqlite3
import sys
import urllib.parse
from collections import Counter, defaultdict

ROOT = os.environ.get("MORPHO_ROOT", os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

WIKTIONARY = "Wiktionary (via dictionaryapi.dev)"
WIKTIONARY_LICENCE = "CC BY-SA 3.0"
OWN = "Morpho authors"
OWN_LICENCE = "CC BY-NC-SA 4.0"


def wiktionary_url(word: str) -> str:
    return "https://en.wiktionary.org/wiki/" + urllib.parse.quote(word.replace(" ", "_"))


def example_credit(word: str, source: str, ref: str, created_by: str) -> tuple[str, str, str]:
    """Source, reference and licence of one example candidate."""
    if source == "tatoeba":
        # "tatoeba:<sentence id>; <licence>"
        head, _, licence = ref.partition(";")
        sid = head.split(":", 1)[-1].strip()
        return "Tatoeba", f"https://tatoeba.org/en/sentences/show/{sid}", licence.strip() or "CC BY 2.0 FR"
    if source == "freedict":
        return WIKTIONARY, wiktionary_url(word), WIKTIONARY_LICENCE
    if created_by == "admin:operator-subs":
        return "OpenSubtitles corpus (OPUS)", "https://opus.nlpl.eu/OpenSubtitles.php", "no licence stated"
    return OWN, "", OWN_LICENCE


def engine_credit(source: str, ref: str, licence: str) -> tuple[str, str, str, str]:
    """A picture the engine fetched: "<source>:<id>" and "<licence>; by <author> (<site>)"."""
    ident = ref.split(":", 1)[-1]
    if source == "openverse":
        name, url = "Openverse", f"https://openverse.org/image/{ident}"
    elif source == "wikimedia":
        name, url = "Wikimedia Commons", "https://commons.wikimedia.org/wiki/" + urllib.parse.quote(ident.replace(" ", "_"))
    elif source == "pixabay":
        name, url = "Pixabay", f"https://pixabay.com/images/id-{ident}/"
    else:
        name, url = source, ""
    lic, _, by = licence.partition("; by ")
    author = by.rsplit(" (", 1)[0] if by else ""
    return name, url, author, lic.strip() or "not recorded"


def upload_credits(db: sqlite3.Connection, log: str) -> dict[int, tuple[str, str, str, str]]:
    """Pictures uploaded by the widening script: its log lists each word's uploads in order."""
    if not os.path.exists(log):
        return {}
    uploads = defaultdict(list)
    with open(log) as f:
        for line in f:
            r = json.loads(line)
            uploads[r["word_id"]] += [u for u in r["uploads"] if u.get("status") == "201"]
    site = {"openverse": "Openverse", "wikimedia": "Wikimedia Commons", "pixabay": "Pixabay"}
    out = {}
    cands = defaultdict(list)
    for cid, wid in db.execute(
        "SELECT img_cand_id, word_id FROM image_candidates WHERE created_by = 'admin:expand-images' ORDER BY img_cand_id"
    ):
        cands[wid].append(cid)
    for wid, ids in cands.items():
        # One more upload than candidates: two pages served the same file, stored once.
        for cid, u in zip(ids, uploads.get(wid, [])):
            out[cid] = (site.get(u["source"], u["source"]), u.get("page", ""), u.get("author", ""), u.get("license", ""))
    return out


def coco_credits(accepted: str, annotations: list[str], cache: str) -> dict[int, tuple[str, str, str, str]]:
    """COCO pictures by word: the image id accepted for it, its Flickr page and licence.

    Read from the COCO 2014 annotations when given, which also refreshes `cache`
    (content/attribution/coco.tsv); otherwise from that cache.
    """
    site = "COCO 2014 (Flickr)"
    rows = []
    if annotations and os.path.exists(accepted):
        info, names = {}, {}
        for path in annotations:
            with open(path) as f:
                ann = json.load(f)
            names.update({lic["id"]: lic["name"] for lic in ann["licenses"]})
            info.update({img["id"]: img for img in ann["images"]})
        with open(accepted) as f:
            for a in json.load(f):
                img = info.get(a["image_id"], {})
                rows.append((a["word_id"], a["image_id"], img.get("flickr_url", ""), names.get(img.get("license"), "")))
        with open(cache, "w", newline="") as f:
            out = csv.writer(f, delimiter="\t", lineterminator="\n")
            out.writerow(["word_id", "coco_image_id", "flickr_url", "licence"])
            out.writerows(sorted(rows))
    elif os.path.exists(cache):
        with open(cache) as f:
            rows = [(int(r["word_id"]), r["coco_image_id"], r["flickr_url"], r["licence"]) for r in csv.DictReader(f, delimiter="\t")]
    return {w: (site, url or f"COCO image {iid}", "", lic or "see the COCO 2014 annotations") for w, iid, url, lic in rows}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--release", default=f"{ROOT}/app/app/src/main/assets/release.db")
    ap.add_argument("--db", default=os.environ.get("MORPHO_DB", f"{ROOT}/data/working.db"))
    ap.add_argument("--out", default=f"{ROOT}/content/attribution")
    ap.add_argument("--expand-log", default=f"{ROOT}/ops/logs/expand_images.jsonl")
    ap.add_argument("--coco-accepted", default=f"{ROOT}/ops/logs/coco-accepted.json")
    ap.add_argument("--coco", nargs="*", default=[], help="COCO 2014 captions_*.json annotation files")
    args = ap.parse_args()

    con = sqlite3.connect(f"file:{args.db}?mode=ro", uri=True)
    con.execute("ATTACH ? AS r", (f"file:{args.release}?mode=ro",))
    os.makedirs(args.out, exist_ok=True)

    # A shipped sentence may match several candidates (the same text fetched twice): prefer a
    # third-party one, so its credit is never lost.
    rows = con.execute(
        """
        SELECT e.word_id, w.word, e.sentence, c.source, COALESCE(c.source_ref, ''), COALESCE(c.created_by, '')
        FROM r.examples e
        JOIN r.words w ON w.word_id = e.word_id
        LEFT JOIN example_candidates c ON c.word_id = e.word_id AND c.text = e.sentence
        ORDER BY w.learning_order, e.display_order, (c.source = 'manual')
        """
    ).fetchall()
    seen, counts, unmatched = set(), Counter(), 0
    with open(f"{args.out}/examples.tsv", "w", newline="") as f:
        out = csv.writer(f, delimiter="\t", lineterminator="\n")
        out.writerow(["word_id", "word", "sentence", "source", "reference", "licence"])
        for word_id, word, sentence, source, ref, created_by in rows:
            if (word_id, sentence) in seen:
                continue
            seen.add((word_id, sentence))
            if source is None:
                unmatched += 1
                credit = (OWN, "", OWN_LICENCE)
            else:
                credit = example_credit(word, source, ref, created_by)
            counts[credit[0]] += 1
            out.writerow([word_id, word, sentence, *credit])

    etym = con.execute(
        """
        SELECT rw.word_id, rw.word, COALESCE(w.etymology_source, '')
        FROM r.words rw LEFT JOIN words w ON w.word_id = rw.word_id
        WHERE COALESCE(rw.etymology, '') <> ''
        ORDER BY rw.learning_order
        """
    ).fetchall()
    etym_counts = Counter()
    with open(f"{args.out}/etymology.tsv", "w", newline="") as f:
        out = csv.writer(f, delimiter="\t", lineterminator="\n")
        out.writerow(["word_id", "word", "source", "reference", "licence"])
        for word_id, word, source in etym:
            if source == "wiktionary":
                credit = ("Wiktionary", wiktionary_url(word), WIKTIONARY_LICENCE)
            else:
                credit = (OWN, "", OWN_LICENCE)
            etym_counts[credit[0]] += 1
            out.writerow([word_id, word, *credit])

    uploads = upload_credits(con, args.expand_log)
    coco = coco_credits(args.coco_accepted, args.coco, f"{args.out}/coco.tsv")
    pictures = con.execute(
        """
        SELECT e.word_id, w.word, e.image_file, c.img_cand_id, c.source, COALESCE(c.source_ref, ''),
               COALESCE(c.license, ''), COALESCE(c.created_by, '')
        FROM r.examples e
        JOIN r.words w ON w.word_id = e.word_id
        LEFT JOIN image_candidates c ON 'img/' || c.file_hash || '.webp' = e.image_file AND c.word_id = e.word_id
        WHERE COALESCE(e.image_file, '') <> ''
        ORDER BY w.learning_order, e.display_order
        """
    ).fetchall()
    pic_counts, seen_files = Counter(), set()
    with open(f"{args.out}/images.tsv", "w", newline="") as f:
        out = csv.writer(f, delimiter="\t", lineterminator="\n")
        out.writerow(["word_id", "word", "file", "source", "reference", "author", "licence"])
        for word_id, word, file, cid, source, ref, licence, created_by in pictures:
            if file in seen_files:
                continue
            seen_files.add(file)
            if cid is None or created_by not in ("admin:expand-images", "admin:local") and created_by.startswith("admin:"):
                credit = ("not recorded", "", "", "not recorded")
            elif created_by == "admin:expand-images":
                credit = uploads.get(cid, ("not recorded", "", "", "not recorded"))
            elif created_by == "admin:local":
                credit = coco.get(word_id, ("not recorded", "", "", "not recorded"))
            else:
                credit = engine_credit(source, ref, licence)
            pic_counts[credit[0]] += 1
            out.writerow([word_id, word, file, *credit])

    print(f"examples: {dict(counts)}" + (f", {unmatched} not traced (credited to Morpho authors)" if unmatched else ""))
    print(f"etymology: {dict(etym_counts)}")
    print(f"images: {dict(pic_counts)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
