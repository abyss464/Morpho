#!/usr/bin/env python3
"""Mint authored definitions and slot-1 examples, select, pin and approve them.

Reads content/definitions/*.txt (format in ops/defcheck.py). For every word in
the files, through the admin API:

  * each authored (pos, definition) is minted as a manual candidate (an
    identical existing candidate is reused), selected into that pos slot
    (human, pinned) and approved; a pos slot that does not exist yet is created
    by the selection;
  * the authored primary pos becomes the primary slot;
  * every other enabled slot of the word is disabled, so the word shows exactly
    the authored senses;
  * the authored example is minted (source tag `llm`), selected into slot 1
    (human, pinned) and approved, then read back, because a fresh example mint
    can be filed into another slot by the reconciler (OPERATIONS.md §7.5).

Run ops/defcheck.py on the files first. The word's picture was chosen for the
old slot-1 sentence; `--release-images` re-selects the current picture unpinned
and un-approves it, so automatic selection re-ranks the pool against the new
sentence by CLIP (re-approve with ops/bulk_approve.py once it has converged).

  ops/mint_authored.py FILE... [--words ID,ID] [--release-images] [--dry-run]

MORPHO_API defaults to the dockerised engine, http://127.0.0.1:30012.
"""

import argparse
import json
import os
import re
import sqlite3
import sys
import unicodedata
import urllib.error
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012").rstrip("/") + "/api"
DB = os.environ.get("MORPHO_DB", os.path.join(ROOT, "data/working.db"))
USER = "authored-definitions"
POS = {"n": "noun", "v": "verb", "adj": "adj", "adv": "adv", "prep": "prep", "conj": "conj", "interj": "interj", "phrase": "phrase"}


def canon(text):
    return " ".join(unicodedata.normalize("NFC", text).split())


def api(method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(
        API + path, data=data, method=method,
        headers={"Content-Type": "application/json", "X-Morpho-User": USER},
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return resp.status, None
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode(errors="replace")[:300]
    except urllib.error.URLError as e:
        return 0, str(e.reason)


def read_files(paths):
    words = {}
    for path in paths:
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.rstrip("\n")
                if not line.strip() or line.startswith("#"):
                    continue
                parts = [p.strip() for p in line.split(" | ")]
                wid, pos, primary, text = int(parts[0]), POS[parts[1]], parts[2] == "*", parts[3]
                entry = words.setdefault(wid, {"senses": [], "primary": None, "example": None})
                entry["senses"].append((pos, canon(text)))
                if primary:
                    entry["primary"] = pos
                    entry["example"] = canon(parts[4])
    return words


def highlight(lemma, sentence):
    """Byte offsets of the first inflected occurrence; advisory, the store re-locates."""
    stem = re.escape(lemma[:-1] if lemma.endswith(("e", "y")) and len(lemma) > 3 else lemma)
    m = re.search(rf"\b{stem}\w*", sentence, re.IGNORECASE)
    if not m:
        return 0, 0
    return len(sentence[: m.start()].encode()), len(sentence[: m.end()].encode())


class Word:
    def __init__(self, conn, wid):
        self.conn, self.wid = conn, wid
        self.lemma = conn.execute("SELECT lemma FROM words WHERE word_id = ?", (wid,)).fetchone()[0]

    def slots(self):
        return {
            pos: (cand, enabled, primary, approved)
            for pos, cand, enabled, primary, approved in self.conn.execute(
                "SELECT pos, def_cand_id, enabled, is_primary, approved FROM definition_selections WHERE word_id = ?",
                (self.wid,),
            )
        }

    def def_cand(self, pos, text):
        row = self.conn.execute(
            "SELECT def_cand_id FROM definition_candidates WHERE word_id = ? AND pos = ? AND text = ? AND status = 'available'",
            (self.wid, pos, text),
        ).fetchone()
        return row[0] if row else None

    def ex_cand(self, text):
        row = self.conn.execute(
            "SELECT ex_cand_id FROM example_candidates WHERE word_id = ? AND text = ? AND status = 'available'",
            (self.wid, text),
        ).fetchone()
        return row[0] if row else None

    def ex_slot1(self):
        row = self.conn.execute(
            "SELECT ex_cand_id, approved FROM example_selections WHERE word_id = ? AND slot = 1", (self.wid,)
        ).fetchone()
        return row if row else (None, 0)

    def ex_other_slot(self, cand):
        row = self.conn.execute(
            "SELECT slot FROM example_selections WHERE word_id = ? AND ex_cand_id = ? AND slot != 1",
            (self.wid, cand),
        ).fetchone()
        return row[0] if row else None

    def ex_spare(self, cand):
        row = self.conn.execute(
            "SELECT ec.ex_cand_id FROM example_candidates ec WHERE ec.word_id = ? AND ec.status = 'available'"
            " AND ec.ex_cand_id != ? AND NOT EXISTS (SELECT 1 FROM example_selections es"
            " WHERE es.word_id = ec.word_id AND es.ex_cand_id = ec.ex_cand_id) ORDER BY ec.ex_cand_id LIMIT 1",
            (self.wid, cand),
        ).fetchone()
        return row[0] if row else None


def release_image(conn, wid, call):
    row = conn.execute("SELECT img_cand_id, pinned, approved FROM image_selections WHERE word_id = ?", (wid,)).fetchone()
    if row is None:
        return
    cand, pinned, approved = row
    if pinned:
        call("POST", "/selections/image", {"word_id": wid, "cand_id": cand, "pin": False})
    if approved:
        call("DELETE", "/selections/image/approve", {"word_id": wid})


def mint_word(conn, wid, spec, dry, release_images=False):
    w = Word(conn, wid)
    errors = []

    def call(method, path, body):
        if dry:
            print(f"  DRY {method} {path} {json.dumps(body, ensure_ascii=False)}")
            return True
        status, err = api(method, path, body)
        if status not in (200, 201):
            errors.append(f"{method} {path} -> {status} {err}")
            return False
        return True

    for pos, text in spec["senses"]:
        cand = w.def_cand(pos, text)
        if cand is None:
            if not call("POST", "/candidates/definition", {"word_id": wid, "pos": pos, "text": text}):
                continue
            cand = w.def_cand(pos, text)
            if cand is None and not dry:
                errors.append(f"definition candidate for {pos} not found after mint")
                continue
        current = w.slots().get(pos)
        if current is None or current[0] != cand:
            call("POST", "/selections/definition", {"word_id": wid, "pos": pos, "cand_id": cand or 0})
        if current is None or current[0] != cand or not current[3]:
            call("POST", "/selections/definition/approve", {"word_id": wid, "pos": pos})
        current = w.slots().get(pos)
        if current is not None and not current[1]:
            call("POST", "/selections/definition/enabled", {"word_id": wid, "pos": pos, "enabled": True})

    slots = w.slots()
    if spec["primary"] and not slots.get(spec["primary"], (0, 0, 0, 0))[2]:
        call("POST", "/selections/definition/primary", {"word_id": wid, "pos": spec["primary"]})
    authored = {pos for pos, _ in spec["senses"]}
    for pos, (_, enabled, _, _) in w.slots().items():
        if enabled and pos not in authored:
            call("POST", "/selections/definition/enabled", {"word_id": wid, "pos": pos, "enabled": False})

    if spec["example"]:
        text = spec["example"]
        cand = w.ex_cand(text)
        if cand is None:
            start, end = highlight(w.lemma, text)
            if call("POST", "/candidates/example", {"word_id": wid, "text": text, "hl_start": start, "hl_end": end, "source": "llm"}):
                cand = w.ex_cand(text)
        if cand is not None or dry:
            slot_cand, approved = w.ex_slot1()
            other = w.ex_other_slot(cand) if cand is not None else None
            if slot_cand != cand and other is not None:
                # The authored sentence already exists as this word's example
                # in another slot; hand that slot a spare so slot 1 can take it.
                spare = w.ex_spare(cand)
                if spare is None:
                    errors.append(f"authored example fills slot {other} and the word has no spare example")
                else:
                    call("POST", "/selections/example", {"word_id": wid, "slot": other, "cand_id": spare, "pin": False})
            if slot_cand != cand:
                call("POST", "/selections/example", {"word_id": wid, "slot": 1, "cand_id": cand or 0})
            if slot_cand != cand or not approved:
                call("POST", "/selections/example/approve", {"word_id": wid, "slot": 1})
            if not dry and w.ex_slot1()[0] != cand:
                errors.append("slot 1 does not hold the authored example after selection")
        else:
            errors.append("example candidate not found after mint")
    if release_images:
        release_image(conn, wid, call)
    return w.lemma, errors


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("files", nargs="+")
    ap.add_argument("--words", help="comma-separated word_ids to restrict to")
    ap.add_argument("--release-images", action="store_true", help="unpin and un-approve the current picture")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    words = read_files(args.files)
    if args.words:
        keep = {int(x) for x in args.words.split(",")}
        words = {k: v for k, v in words.items() if k in keep}
    conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    failed = 0
    for n, (wid, spec) in enumerate(sorted(words.items()), 1):
        lemma, errors = mint_word(conn, wid, spec, args.dry_run, args.release_images)
        if errors:
            failed += 1
            print(f"{wid} {lemma}: " + " | ".join(errors))
        if n % 100 == 0:
            print(f"... {n}/{len(words)}", flush=True)
    print(f"{len(words)} words, {failed} with errors")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
