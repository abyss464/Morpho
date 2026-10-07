#!/usr/bin/env python3
"""Write the credits for third-party text shipped in the app's release.db (NOTICE.md).

Every example sentence in the release is traced back to its candidate in the working
database, and every etymology to its source, and listed with where it came from and under
which licence:

  content/attribution/examples.tsv   word_id, word, sentence, source, reference, licence
  content/attribution/etymology.tsv  word_id, word, source, reference, licence

ops/release.sh runs it after copying a new release.db. Read-only on both databases.

    python3 ops/attribution.py [--release PATH] [--db PATH] [--out DIR]
"""

import argparse
import csv
import os
import sqlite3
import sys
import urllib.parse
from collections import Counter

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


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--release", default=f"{ROOT}/app/app/src/main/assets/release.db")
    ap.add_argument("--db", default=os.environ.get("MORPHO_DB", f"{ROOT}/data/working.db"))
    ap.add_argument("--out", default=f"{ROOT}/content/attribution")
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

    print(f"examples: {dict(counts)}" + (f", {unmatched} not traced (credited to Morpho authors)" if unmatched else ""))
    print(f"etymology: {dict(etym_counts)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
