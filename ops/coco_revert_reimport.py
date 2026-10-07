#!/usr/bin/env python3
"""Revert and re-import the COCO-accepted (image, sentence) pairs.

Background: ops/coco_ingest.py minted example candidates with CLIENT-computed
byte offsets for the highlighted lemma. The engine has since gained
server-side offset recomputation (POST /candidates/example now ignores the
client hl_start/hl_end and locates the word itself; 422 if it cannot find it)
plus a reconciler sweep that auto-releases/re-selects any slot pinned to a
non-available candidate. This script forces every COCO example candidate
through the new path -- reject + purge the old row, then re-mint an
identical-text replacement and re-select it into slot 1 -- and separately
verifies each word's image selection still points at its own COCO upload
(image candidates from the original ingest are never rejected or purged,
only the *selection* is corrected if needed).

Ground truth: ops/logs/coco-accepted.json (2927 entries). A candidate row
matches an entry when example_candidates.word_id = entry.word_id AND
text = canonicalize(caption); canonicalize = NFC -> trim -> collapse internal
whitespace runs to a single space (case preserved) -- the same function
morphod applies server-side, replicated here so text-equality matching works
without a round trip.

The COCO image candidate for a word is
  image_candidates WHERE word_id=? AND source='manual' AND source_ref='upload by local'
(coco_ingest.py always uploaded as actor "local" with no other source_ref).
One word in the current DB has two such rows (an unrelated pre-batch upload
plus the real batch one); pick_coco_image() disambiguates by proximity to the
matched example candidate's created_at, falling back to the newest row.

Subcommands (run in order shown; each re-reads live state, so re-running any
of them after a partial failure or an engine sweep is safe):

  match             Read-only. Resolves every entry to its current matched
                     example candidate (if any) and COCO image candidate,
                     prints a summary, and saves ops/logs/coco-match-state.json.
                     Safe to run at any time, including mid-mission, to check
                     convergence.
  reject            3(a). POST reject on the matched example candidate for
                     every entry whose candidate is still 'available'.
                     Throttled ~10/s.
  purge             3(b). DELETE-purge every matched example candidate whose
                     status is 'rejected'. 409 means a slot still points at
                     it (wait a sweep, re-run purge to retry).
  reimport          4. For every entry (including the never-minted ones):
                     find-or-mint the example candidate, select+approve slot
                     1, then verify/fix the word's image selection against
                     its COCO upload.
  fix-slot-conflicts  Follow-up (dev/wave-1 d5be95e): re-minting a rejected
                     candidate now revives it in place (status->available,
                     offsets recomputed) instead of returning it inertly.
                     That fixes slot-1-stuck words for free on a plain
                     `reimport` rerun, but a word whose matched candidate is
                     revived while still sitting in a DIFFERENT slot (2 or 3)
                     can't be selected into slot 1 directly -- UNIQUE(word_id,
                     ex_cand_id) already holds that pair, so the select 409s.
                     Two phases for that residual set:
                       --phase reject   reject the candidate to free the slot
                                        it currently (still) occupies
                       --phase finish   (after an external sweep so the freed
                                        slot refills) re-mint it again -- now
                                        unreferenced, so it revives cleanly --
                                        then select it into slot 1 and approve
                     Takes a JSON list of {word_id, lemma, slot, ex_cand_id}
                     (e.g. the slot != 1 rows of coco-revert-unpurged-stuck.json).

Env overrides:
  MORPHO_API    admin API base (default http://127.0.0.1:30012/api)
  MORPHO_DB     path to working.db (default <repo>/data/working.db)
  MORPHO_USER   X-Morpho-User header (default "ops")
"""

import argparse
import datetime
import json
import os
import sqlite3
import time
import unicodedata

import requests

ROOT = os.environ.get("MORPHO_ROOT", os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012/api")
DB = os.environ.get("MORPHO_DB", f"{ROOT}/data/working.db")
ACTOR = os.environ.get("MORPHO_USER", "ops")
INPUT_PATH = f"{ROOT}/ops/logs/coco-accepted.json"
LOG_DIR = f"{ROOT}/ops/logs"
HEADERS = {"X-Morpho-User": ACTOR}
THROTTLE_S = 0.1  # ~10/s


def canonicalize(text):
    """Replicate morphod's canonicalize: NFC + trim + collapse whitespace."""
    text = unicodedata.normalize("NFC", text)
    out = []
    pending_space = False
    for ch in text:
        if ch.isspace() or unicodedata.category(ch) == "Zs":
            if out:
                pending_space = True
        else:
            if pending_space:
                out.append(" ")
                pending_space = False
            out.append(ch)
    return "".join(out)


def ro_conn():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def load_entries():
    return json.load(open(INPUT_PATH))


def in_clause(word_ids):
    return ",".join("?" * len(word_ids))


def load_example_index(conn, word_ids):
    """word_id -> list of (ex_cand_id, text, status)."""
    idx = {}
    q = f"SELECT word_id, ex_cand_id, text, status FROM example_candidates WHERE word_id IN ({in_clause(word_ids)})"
    for wid, cid, text, status in conn.execute(q, word_ids):
        idx.setdefault(wid, []).append((cid, text, status))
    return idx


def load_image_index(conn, word_ids):
    """word_id -> list of (img_cand_id, status, created_at) for source='manual', source_ref='upload by local'."""
    idx = {}
    q = (
        f"SELECT word_id, img_cand_id, status, created_at FROM image_candidates "
        f"WHERE word_id IN ({in_clause(word_ids)}) AND source='manual' AND source_ref='upload by local'"
    )
    for wid, cid, status, created in conn.execute(q, word_ids):
        idx.setdefault(wid, []).append((cid, status, created))
    return idx


def load_example_slot1(conn, word_ids):
    idx = {}
    q = f"SELECT word_id, ex_cand_id FROM example_selections WHERE word_id IN ({in_clause(word_ids)}) AND slot=1"
    for wid, cid in conn.execute(q, word_ids):
        idx[wid] = cid
    return idx


def load_image_selection(conn, word_ids):
    idx = {}
    q = f"SELECT word_id, img_cand_id FROM image_selections WHERE word_id IN ({in_clause(word_ids)})"
    for wid, cid in conn.execute(q, word_ids):
        idx[wid] = cid
    return idx


def _ts(s):
    return datetime.datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()


def pick_coco_image(rows, example_created_at=None):
    """rows: list of (img_cand_id, status, created_at). Disambiguate >1 row."""
    if not rows:
        return None
    if len(rows) == 1:
        return rows[0]
    if example_created_at:
        return min(rows, key=lambda r: abs(_ts(r[2]) - _ts(example_created_at)))
    return max(rows, key=lambda r: r[2])


def find_text_match(rows, text):
    return next((r for r in rows if r[1] == text), None)


def api(method, path, body=None, timeout=30):
    return requests.request(method, f"{API}{path}", headers=HEADERS, json=body, timeout=timeout)


def save_json(name, data):
    os.makedirs(LOG_DIR, exist_ok=True)
    path = f"{LOG_DIR}/{name}"
    with open(path, "w") as f:
        json.dump(data, f, indent=1)
    print(f"  -> {path}")
    return path


# ---------------------------------------------------------------------------
# match -- read-only diagnostics / convergence check
# ---------------------------------------------------------------------------

def cmd_match(args):
    conn = ro_conn()
    entries = load_entries()
    word_ids = [e["word_id"] for e in entries]
    ex_idx = load_example_index(conn, word_ids)
    im_idx = load_image_index(conn, word_ids)
    slot1 = load_example_slot1(conn, word_ids)
    q = f"SELECT word_id, slot, ex_cand_id FROM example_selections WHERE word_id IN ({in_clause(word_ids)})"
    all_slots = {}
    for wid, slot, cid in conn.execute(q, word_ids):
        all_slots.setdefault(wid, {})[slot] = cid
    img_sel = load_image_selection(conn, word_ids)

    no_candidate = []
    matched = []
    for e in entries:
        wid, lemma, caption = e["word_id"], e["lemma"], e["caption"]
        text = canonicalize(caption)
        row = find_text_match(ex_idx.get(wid, []), text)
        if row is None:
            no_candidate.append({"word_id": wid, "lemma": lemma, "caption": caption})
            continue
        cid, _, status = row
        slot = next((s for s, c in all_slots.get(wid, {}).items() if c == cid), None)
        coco_img = pick_coco_image(im_idx.get(wid, []))
        matched.append({
            "word_id": wid,
            "lemma": lemma,
            "ex_cand_id": cid,
            "ex_status": status,
            "ex_slot": slot,
            "coco_img_cand_id": coco_img[0] if coco_img else None,
            "coco_img_status": coco_img[1] if coco_img else None,
            "current_img_cand_id": img_sel.get(wid),
            "image_matches_coco": coco_img is not None and img_sel.get(wid) == coco_img[0],
        })

    n_slot1 = sum(1 for m in matched if m["ex_slot"] == 1)
    n_slot23 = sum(1 for m in matched if m["ex_slot"] in (2, 3))
    n_unselected = sum(1 for m in matched if m["ex_slot"] is None)
    n_rejected = sum(1 for m in matched if m["ex_status"] == "rejected")
    img_mismatch = [m for m in matched if not m["image_matches_coco"]]

    print(f"entries={len(entries)} matched={len(matched)} no_candidate={len(no_candidate)}")
    print(f"  ex_slot1={n_slot1} ex_slot2_3={n_slot23} ex_unselected={n_unselected} ex_status_rejected={n_rejected}")
    print(f"  image selection != coco upload: {len(img_mismatch)}")
    for m in img_mismatch:
        print(f"    word {m['word_id']:>6} {m['lemma']:<16} selected={m['current_img_cand_id']} "
              f"coco={m['coco_img_cand_id']} (coco_status={m['coco_img_status']})")
    if no_candidate:
        print("  never-minted:")
        for n in no_candidate:
            print(f"    word {n['word_id']:>6} {n['lemma']!r}")

    save_json("coco-match-state.json", {"matched": matched, "no_candidate": no_candidate})


# ---------------------------------------------------------------------------
# reject -- stage 3(a)
# ---------------------------------------------------------------------------

def cmd_reject(args):
    conn = ro_conn()
    entries = load_entries()
    word_ids = [e["word_id"] for e in entries]
    ex_idx = load_example_index(conn, word_ids)

    rejected = already_rejected = no_candidate = failed = 0
    failures = []
    for i, e in enumerate(entries):
        wid = e["word_id"]
        text = canonicalize(e["caption"])
        row = find_text_match(ex_idx.get(wid, []), text)
        if row is None:
            no_candidate += 1
            continue
        cid, _, status = row
        if status == "rejected":
            already_rejected += 1
            continue
        if args.dry_run:
            rejected += 1
            continue
        r = api("POST", f"/candidates/example/{cid}/reject", {})
        if r.status_code < 400:
            rejected += 1
        else:
            failed += 1
            failures.append({"word_id": wid, "ex_cand_id": cid, "status_code": r.status_code, "body": r.text[:300]})
        time.sleep(THROTTLE_S)
        if (i + 1) % 300 == 0:
            print(f"  ...{i + 1}/{len(entries)} (rejected={rejected} already={already_rejected} failed={failed})")

    tag = " [dry-run]" if args.dry_run else ""
    print(f"DONE reject{tag}: rejected={rejected} already_rejected={already_rejected} "
          f"no_candidate(skip)={no_candidate} failed={failed}")
    save_json("coco-revert-reject-results.json", {
        "dry_run": args.dry_run,
        "rejected": rejected, "already_rejected": already_rejected,
        "no_candidate": no_candidate, "failed": failed, "failures": failures,
    })


# ---------------------------------------------------------------------------
# purge -- stage 3(b)
# ---------------------------------------------------------------------------

def cmd_purge(args):
    conn = ro_conn()
    entries = load_entries()
    word_ids = [e["word_id"] for e in entries]
    ex_idx = load_example_index(conn, word_ids)

    to_purge = []
    for e in entries:
        wid = e["word_id"]
        text = canonicalize(e["caption"])
        row = find_text_match(ex_idx.get(wid, []), text)
        if row is None:
            continue
        cid, _, status = row
        if status == "rejected":
            to_purge.append((wid, cid))

    purged = 0
    conflict_409 = []
    other_fail = []
    for wid, cid in to_purge:
        if args.dry_run:
            purged += 1
            continue
        r = api("DELETE", f"/candidates/example/{cid}")
        if r.status_code < 400:
            purged += 1
        elif r.status_code == 409:
            conflict_409.append({"word_id": wid, "ex_cand_id": cid})
        else:
            other_fail.append({"word_id": wid, "ex_cand_id": cid, "status_code": r.status_code, "body": r.text[:300]})
        time.sleep(THROTTLE_S)

    tag = " [dry-run]" if args.dry_run else ""
    print(f"DONE purge{tag}: candidates_to_purge={len(to_purge)} purged={purged} "
          f"409={len(conflict_409)} other_fail={len(other_fail)}")
    save_json("coco-revert-purge-results.json", {
        "dry_run": args.dry_run,
        "candidates_to_purge": len(to_purge), "purged": purged,
        "conflict_409": conflict_409, "other_fail": other_fail,
    })


# ---------------------------------------------------------------------------
# reimport -- stage 4
# ---------------------------------------------------------------------------

def cmd_reimport(args):
    conn = ro_conn()
    entries = load_entries()
    word_ids = [e["word_id"] for e in entries]
    ex_idx = load_example_index(conn, word_ids)
    im_idx = load_image_index(conn, word_ids)
    slot1 = load_example_slot1(conn, word_ids)
    img_sel = load_image_selection(conn, word_ids)

    minted = reused = 0
    mint_exceptions = []
    selected = already_selected = 0
    select_exceptions = []
    approved = approve_409 = 0
    img_fixed = img_already_ok = img_missing = 0
    img_exceptions = []

    for i, e in enumerate(entries):
        wid, lemma, caption = e["word_id"], e["lemma"], e["caption"]
        text = canonicalize(caption)

        row = find_text_match(ex_idx.get(wid, []), text)
        if row is not None and row[2] == "available":
            # A genuinely usable candidate with this exact text already exists
            # (e.g. purge never ran, or ran and left this one alone).
            cid = row[0]
            reused += 1
        else:
            if row is not None:
                # Matched text exists but is NOT available -- almost always a
                # purge-409 leftover (status='rejected', still referenced by a
                # slot, so purge couldn't remove it). Minting the identical
                # text would collide with this row's UNIQUE(word_id,
                # text_hash) at the DB level; do not silently treat the stale
                # row as reusable. Attempt the mint anyway so the server's
                # real response (idempotent return vs. conflict error) is
                # captured precisely rather than guessed at, and record which
                # case this is.
                pass
            if args.dry_run:
                minted += 1
                cid = None
            else:
                r = api("POST", "/candidates/example",
                        {"word_id": wid, "text": caption, "source": "coco", "hl_start": 0, "hl_end": 0})
                if r.status_code == 422:
                    mint_exceptions.append({"word_id": wid, "lemma": lemma, "status_code": 422, "body": r.text[:300],
                                             "blocked_by_unpurged": row is not None,
                                             "stale_ex_cand_id": row[0] if row else None})
                    continue
                if r.status_code >= 400:
                    mint_exceptions.append({"word_id": wid, "lemma": lemma, "status_code": r.status_code, "body": r.text[:300],
                                             "blocked_by_unpurged": row is not None,
                                             "stale_ex_cand_id": row[0] if row else None})
                    continue
                wd = r.json()
                cands = wd["examples"][0]["candidates"] if wd.get("examples") else []
                match = next((c for c in cands if c["text"] == text), None)
                if match is None:
                    mint_exceptions.append({"word_id": wid, "lemma": lemma, "error": "minted but not found in response"})
                    continue
                cid = match["ex_cand_id"]
                minted += 1

        if args.dry_run:
            selected += 1
            approved += 1
        else:
            if slot1.get(wid) == cid:
                already_selected += 1
            else:
                r = api("POST", "/selections/example", {"word_id": wid, "slot": 1, "cand_id": cid})
                if r.status_code >= 400:
                    select_exceptions.append({"word_id": wid, "lemma": lemma, "ex_cand_id": cid,
                                               "status_code": r.status_code, "body": r.text[:300]})
                    time.sleep(THROTTLE_S)
                    continue
                selected += 1

            r = api("POST", "/selections/example/approve", {"word_id": wid, "slot": 1})
            if r.status_code == 409:
                approve_409 += 1
            elif r.status_code < 400:
                approved += 1

        # verify / fix image selection against the COCO upload
        coco_img = pick_coco_image(im_idx.get(wid, []))
        if coco_img is None:
            img_missing += 1
        else:
            coco_cid, coco_status, _ = coco_img
            if img_sel.get(wid) == coco_cid:
                img_already_ok += 1
            elif args.dry_run:
                img_fixed += 1
            else:
                r = api("POST", "/selections/image", {"word_id": wid, "cand_id": coco_cid})
                if r.status_code >= 400:
                    img_exceptions.append({
                        "word_id": wid, "lemma": lemma, "coco_img_cand_id": coco_cid,
                        "coco_img_status": coco_status, "previous_img_cand_id": img_sel.get(wid),
                        "status_code": r.status_code, "body": r.text[:300],
                    })
                else:
                    api("POST", "/selections/image/approve", {"word_id": wid})
                    img_fixed += 1

        time.sleep(THROTTLE_S)
        if (i + 1) % 200 == 0:
            print(f"  ...{i + 1}/{len(entries)}")

    tag = " [dry-run]" if args.dry_run else ""
    print(f"DONE reimport{tag}:")
    print(f"  example: minted={minted} reused={reused} mint_exceptions={len(mint_exceptions)}")
    print(f"  select:  selected={selected} already_selected={already_selected} select_exceptions={len(select_exceptions)}")
    print(f"  approve: approved={approved} approve_409={approve_409}")
    print(f"  image:   fixed={img_fixed} already_ok={img_already_ok} missing_coco_cand={img_missing} "
          f"exceptions={len(img_exceptions)}")
    save_json("coco-reimport-results.json", {
        "dry_run": args.dry_run,
        "example": {"minted": minted, "reused": reused, "exceptions": mint_exceptions},
        "select": {"selected": selected, "already_selected": already_selected, "exceptions": select_exceptions},
        "approve": {"approved": approved, "approve_409": approve_409},
        "image": {"fixed": img_fixed, "already_ok": img_already_ok, "missing_coco_cand": img_missing,
                   "exceptions": img_exceptions},
    })


# ---------------------------------------------------------------------------
# fix-slot-conflicts -- follow-up leg (d5be95e revival fix). A word whose
# matched candidate got revived (by a `reimport` mint attempt) while still
# referenced by a slot other than 1 can't be moved into slot 1 directly: the
# UNIQUE(word_id, ex_cand_id) constraint already holds that pair. Free it by
# rejecting the candidate (phase "reject"), wait one sweep so the vacated
# slot refills from the rest of the pool, then re-mint (revives it again, now
# unreferenced anywhere), select into slot 1, and approve (phase "finish").
# ---------------------------------------------------------------------------

def cmd_fix_slot_conflicts(args):
    targets = json.load(open(args.words))
    entries_by_word = {e["word_id"]: e for e in load_entries()}

    if args.phase == "reject":
        ok = failed = 0
        results = []
        for t in targets:
            wid, cid = t["word_id"], t["ex_cand_id"]
            r = api("POST", f"/candidates/example/{cid}/reject", {})
            success = r.status_code < 400
            results.append({"word_id": wid, "ex_cand_id": cid, "status_code": r.status_code, "ok": success})
            if success:
                ok += 1
            else:
                failed += 1
            time.sleep(THROTTLE_S)
        print(f"DONE fix-slot-conflicts/reject: rejected={ok} failed={failed}")
        save_json("coco-slotfix-reject-results.json", {"rejected": ok, "failed": failed, "results": results})
        return

    # phase == "finish": re-mint (revives again, now unreferenced), select
    # into slot 1, approve.
    minted = select_ok = approved = 0
    exceptions = []
    for t in targets:
        wid = t["word_id"]
        e = entries_by_word[wid]
        lemma, caption = e["lemma"], e["caption"]
        text = canonicalize(caption)

        r = api("POST", "/candidates/example", {"word_id": wid, "text": caption, "source": "coco", "hl_start": 0, "hl_end": 0})
        if r.status_code >= 400:
            exceptions.append({"word_id": wid, "lemma": lemma, "stage": "mint",
                                "status_code": r.status_code, "body": r.text[:300]})
            continue
        wd = r.json()
        cands = wd["examples"][0]["candidates"] if wd.get("examples") else []
        match = next((c for c in cands if c["text"] == text), None)
        if match is None:
            exceptions.append({"word_id": wid, "lemma": lemma, "stage": "mint",
                                "error": "minted but not found in response"})
            continue
        cid = match["ex_cand_id"]
        minted += 1

        r = api("POST", "/selections/example", {"word_id": wid, "slot": 1, "cand_id": cid})
        if r.status_code >= 400:
            exceptions.append({"word_id": wid, "lemma": lemma, "stage": "select", "ex_cand_id": cid,
                                "status_code": r.status_code, "body": r.text[:300]})
            time.sleep(THROTTLE_S)
            continue
        select_ok += 1

        r = api("POST", "/selections/example/approve", {"word_id": wid, "slot": 1})
        if r.status_code < 400:
            approved += 1
        else:
            exceptions.append({"word_id": wid, "lemma": lemma, "stage": "approve", "ex_cand_id": cid,
                                "status_code": r.status_code, "body": r.text[:300]})
        time.sleep(THROTTLE_S)

    print(f"DONE fix-slot-conflicts/finish: minted={minted} selected={select_ok} approved={approved} "
          f"exceptions={len(exceptions)}")
    save_json("coco-slotfix-finish-results.json", {
        "minted": minted, "selected": select_ok, "approved": approved, "exceptions": exceptions,
    })


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest="mode", required=True)

    p_match = sub.add_parser("match", help="read-only: resolve entries to candidates, print + save state")
    p_match.set_defaults(func=cmd_match)

    p_reject = sub.add_parser("reject", help="3(a): reject the matched available candidate for every entry")
    p_reject.add_argument("--dry-run", action="store_true")
    p_reject.set_defaults(func=cmd_reject)

    p_purge = sub.add_parser("purge", help="3(b): DELETE-purge every matched rejected candidate")
    p_purge.add_argument("--dry-run", action="store_true")
    p_purge.set_defaults(func=cmd_purge)

    p_reimport = sub.add_parser("reimport", help="4: find-or-mint, select+approve slot 1, fix image selection")
    p_reimport.add_argument("--dry-run", action="store_true")
    p_reimport.set_defaults(func=cmd_reimport)

    p_fix = sub.add_parser("fix-slot-conflicts",
                            help="follow-up: free/re-mint/select/approve a revived candidate stuck in a non-1 slot")
    p_fix.add_argument("words", help="JSON list of {word_id, lemma, slot, ex_cand_id}")
    p_fix.add_argument("--phase", choices=["reject", "finish"], required=True)
    p_fix.set_defaults(func=cmd_fix_slot_conflicts)

    args = p.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
