"""Move a word's primary sense onto the part of speech the corpus actually uses.

`scorer/2` shipped rule 6: a primary the reconciler placed follows the evidence,
and when the evidence moves the primary follows it. The rule refuses to move a
primary onto a **disabled** slot, which is the right call — a slot the word does
not show cannot be the sense a learner meets.

That refusal is what strands ~420 words here. An earlier triage pass
(`admin:operator-triage`, `admin:operator-cut`) collapsed every word to a single
enabled sense slot, and the slot it kept mirrors the primary of the day — the
pre-`scorer/2` one. So a word whose corpus-correct part of speech is disabled
has exactly one enabled slot, the wrong one, and rule 6 looks at the word, finds
no enabled slot it would rather have, and leaves it alone. "add" stays a noun,
"attorney" stays a verb.

This script unsticks them, in three passes over the same word list:

1. **enable** the corpus-correct slot, so rule 6 has somewhere to move to;
2. **converge** — wait for a reconciler pass to move the primary. Words whose
   primary the triage pass moved by hand are invisible to rule 6 forever (the
   reconciler corrects its own guesses, never an editor's, and the audit log
   cannot tell a scripted operator from a person), so those get an explicit
   `POST /selections/definition/primary` instead;
3. **disable** the old wrong-part-of-speech slot, but only once the primary has
   been *observed* to sit on the new one. Disabling the slot that still holds
   the primary is the exact bug this script exists to undo.

The ranking is `ranked_pos` from `core/crates/reconcile/src/stages/select.rs`,
reproduced here against the same WordNet files the engine reads: corpus tag
counts from `cntlist.rev` first, then how many senses the harvest recorded, then
source authority, then the lowest candidate id. Lemmas with a part of speech
WordNet does not model (prep/conj/interj) are decided by the harvest alone,
exactly as the engine decides them.

Usage:
    python3 ops/fix_primary_pos.py --dry-run     # report, touch nothing
    python3 ops/fix_primary_pos.py               # the full three-pass run
    python3 ops/fix_primary_pos.py --verify      # re-check the end state only

Reads go straight to `data/working.db` read-only; every write goes through the
admin API.
"""

import argparse
import json
import os
import sqlite3
import sys
import time
import urllib.error
import urllib.request
from collections import defaultdict
from pathlib import Path

# The engine listens on 8787 inside its container; compose publishes it on
# 30012. Override with MORPHO_API when running against a native build.
API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012/api")
ROOT = Path(__file__).resolve().parent.parent
DB = ROOT / "data" / "working.db"
WORDNET = Path(os.environ.get("MORPHO_WORDNET", ROOT / "data" / "wordnet" / "dict"))
ACTOR = "operator-pos"

# A reconciler full pass runs every 60s; give it room for two.
CONVERGE_TIMEOUT = float(os.environ.get("MORPHO_CONVERGE_TIMEOUT", 240))
CONVERGE_POLL = 10.0

# WordNet models four parts of speech. A harvest carrying any other one is
# decided without corpus evidence — see `WnPos::models`.
WN_POS = ("noun", "verb", "adj", "adv")

# `definition_prior`'s ordering, as `ranked_pos` sees it.
SOURCE_RANK = {"manual": 0, "llm_rewrite": 1, "freedict": 2}

# Actors whose slot decisions this script is allowed to overrule: the machine,
# and the two scripted operator passes that caused the damage. Anything else
# that touched a slot is treated as a person and left alone.
MACHINE_ACTORS = {"reconciler", "admin:operator-triage", "admin:operator-cut"}

SENSE_KEY_POS = {"1": "noun", "2": "verb", "3": "adj", "4": "adv", "5": "adj"}
INDEX_POS = {"n": "noun", "v": "verb", "a": "adj", "s": "adj", "r": "adv"}


# ---------------------------------------------------------------------------
# WordNet evidence
# ---------------------------------------------------------------------------


def fold_lemma(lemma):
    """`morpho_domain::canon::fold_lemma`: collapse whitespace, then lowercase."""
    return " ".join(lemma.split()).lower()


def load_wordnet(dict_dir):
    """Corpus tag counts, with the synset-count fallback the engine also uses.

    Returns `(tagged, synsets)`, both keyed by the folded lemma with spaces
    turned into underscores, each mapping a part of speech to a count.
    """
    tagged = defaultdict(lambda: defaultdict(int))
    rev = dict_dir / "cntlist.rev"
    if rev.is_file():
        # `sense_key sense_number tag_cnt`, sense key `lemma%ss_type:...`.
        for line in rev.read_text(encoding="utf-8", errors="replace").splitlines():
            fields = line.split()
            if len(fields) < 3:
                continue
            key, count = fields[0], fields[2]
            if "%" not in key or not count.isdigit():
                continue
            lemma, rest = key.split("%", 1)
            pos = SENSE_KEY_POS.get(rest[:1])
            if pos is None:
                continue
            tagged[lemma.lower()][pos] += int(count)
    else:
        print(f"warning: no {rev}; falling back to synset counts", file=sys.stderr)

    synsets = defaultdict(lambda: defaultdict(set))
    for suffix in ("noun", "verb", "adj", "adv"):
        path = dict_dir / f"index.{suffix}"
        if not path.is_file():
            continue
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.startswith("  ") or not line.strip():
                continue
            entry = parse_index_line(line)
            if entry is None:
                continue
            lemma, pos, offsets = entry
            synsets[lemma][pos].update(offsets)
    return tagged, synsets


def parse_index_line(line):
    """`lemma pos synset_cnt p_cnt [ptrs] sense_cnt tagsense_cnt offsets...`."""
    fields = line.split()
    try:
        lemma = fields[0].lower()
        pos = INDEX_POS.get(fields[1][:1])
        if pos is None:
            return None
        synset_cnt = int(fields[2])
        ptr_cnt = int(fields[3])
        rest = fields[4 + ptr_cnt :]
        offsets = [int(off) for off in rest[2 : 2 + synset_cnt]]
    except (IndexError, ValueError):
        return None
    if len(offsets) != synset_cnt:
        return None
    return lemma, pos, offsets


def pos_frequency(tagged, synsets, lemma):
    """`WordNet::pos_frequency`. Absent from the result means absent from WordNet."""
    key = fold_lemma(lemma).replace(" ", "_")
    if key in tagged:
        return dict(tagged[key])
    return {pos: len(offsets) for pos, offsets in synsets.get(key, {}).items()}


# ---------------------------------------------------------------------------
# Database reads
# ---------------------------------------------------------------------------


def connect():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def read_state(conn, tagged, synsets):
    """Everything one decision needs, per word."""
    lemmas = {
        word_id: lemma
        for word_id, lemma in conn.execute("SELECT word_id, lemma FROM words")
    }

    # The harvest, filtered exactly as `collect_selections` filters it.
    evidence = defaultdict(dict)
    rows = conn.execute(
        "SELECT dc.word_id, dc.pos, dc.def_cand_id, dc.source"
        "  FROM definition_candidates dc"
        "  JOIN active_words w ON w.word_id = dc.word_id AND w.zh_gloss IS NULL"
        " WHERE dc.status = 'available'"
        " ORDER BY dc.word_id, dc.pos, dc.def_cand_id"
    )
    for word_id, pos, cand_id, source in rows:
        bucket = evidence[word_id]
        rank = SOURCE_RANK.get(source, 3)
        entry = bucket.get(pos)
        if entry is None:
            freq = pos_frequency(tagged, synsets, lemmas[word_id]).get(pos, 0)
            bucket[pos] = {
                "pos": pos,
                "wn_frequency": freq,
                "candidates": 1,
                "source_rank": rank,
                "first_cand_id": cand_id,
            }
        else:
            entry["candidates"] += 1
            entry["source_rank"] = min(entry["source_rank"], rank)
            entry["first_cand_id"] = min(entry["first_cand_id"], cand_id)

    # A word carrying a part of speech WordNet models nothing of has no
    # admissible corpus evidence at all.
    for bucket in evidence.values():
        if any(pos not in WN_POS for pos in bucket):
            for entry in bucket.values():
                entry["wn_frequency"] = 0

    slots = defaultdict(dict)
    primaries = {}
    for word_id, pos, enabled, is_primary, selected_by, pinned in conn.execute(
        "SELECT word_id, pos, enabled, is_primary, selected_by, pinned"
        " FROM definition_selections"
    ):
        slots[word_id][pos] = {
            "enabled": bool(enabled),
            "selected_by": selected_by,
            "pinned": bool(pinned),
        }
        if is_primary:
            primaries[word_id] = pos

    return lemmas, evidence, slots, primaries


def ranked_pos(bucket):
    """`select.rs::ranked_pos`: best claim on `is_primary` first."""
    return [
        entry["pos"]
        for entry in sorted(
            bucket.values(),
            key=lambda e: (
                -e["wn_frequency"],
                -e["candidates"],
                e["source_rank"],
                e["first_cand_id"],
            ),
        )
    ]


def hand_moved_words(conn):
    """`select.rs::words_whose_primary_a_human_moved` — invisible to rule 6."""
    moved = set()
    for (entity_id,) in conn.execute(
        "SELECT DISTINCT entity_id FROM events"
        " WHERE action = 'primary_moved' AND actor <> 'reconciler'"
    ):
        head = entity_id.split(":", 1)[0]
        if head.isdigit():
            moved.add(int(head))
    return moved


def last_slot_actor(conn):
    """Who last flipped each slot's enabled bit, `(word_id, pos) -> actor`."""
    latest = {}
    for entity_id, actor in conn.execute(
        "SELECT entity_id, actor FROM events"
        " WHERE action = 'slot_enabled_changed' ORDER BY event_id"
    ):
        word_id, _, pos = entity_id.partition(":")
        if word_id.isdigit():
            latest[(int(word_id), pos)] = actor
    return latest


# ---------------------------------------------------------------------------
# Planning
# ---------------------------------------------------------------------------


def plan(conn, tagged, synsets):
    """Split every disagreeing word into a fix and a set of skips."""
    lemmas, evidence, slots, primaries = read_state(conn, tagged, synsets)
    moved_by_hand = hand_moved_words(conn)
    slot_actor = last_slot_actor(conn)

    fixes, skips, disagreeing = [], [], []
    for word_id, bucket in evidence.items():
        current = primaries.get(word_id)
        if current is None:
            continue
        order = ranked_pos(bucket)
        if not order or order[0] == current:
            continue
        want = order[0]
        disagreeing.append(word_id)

        word_slots = slots.get(word_id, {})
        incumbent = word_slots.get(current)
        target = word_slots.get(want)

        if target is None:
            skips.append((word_id, lemmas[word_id], current, want, "no target slot"))
            continue
        if incumbent and (incumbent["selected_by"] == "human" or incumbent["pinned"]):
            skips.append((word_id, lemmas[word_id], current, want, "human primary"))
            continue
        actor = slot_actor.get((word_id, want))
        if actor is not None and actor not in MACHINE_ACTORS:
            if actor.startswith("admin:"):
                skips.append(
                    (word_id, lemmas[word_id], current, want, f"slot set by {actor}")
                )
                continue
        fixes.append(
            {
                "word_id": word_id,
                "lemma": lemmas[word_id],
                "from_pos": current,
                "to_pos": want,
                "target_enabled": target["enabled"],
                # Rule 6 will never look at this word again; move it by hand.
                "needs_manual_move": word_id in moved_by_hand,
                "retire": [
                    pos
                    for pos, slot in word_slots.items()
                    if pos != want
                    and slot["enabled"]
                    and not slot["pinned"]
                    and slot["selected_by"] != "human"
                ],
            }
        )
    return fixes, skips, disagreeing


# ---------------------------------------------------------------------------
# API writes
# ---------------------------------------------------------------------------


def post(path, body):
    req = urllib.request.Request(
        API + path,
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "X-Morpho-User": ACTOR},
        method="POST",
    )
    try:
        urllib.request.urlopen(req, timeout=60)
        return True
    except urllib.error.HTTPError as err:
        print(f"  {path} {body}: HTTP {err.code}", file=sys.stderr)
        return False
    except urllib.error.URLError as err:
        print(f"  {path} {body}: {err.reason}", file=sys.stderr)
        return False


def set_enabled(word_id, pos, enabled):
    return post(
        "/selections/definition/enabled",
        {"word_id": word_id, "pos": pos, "enabled": enabled},
    )


def set_primary(word_id, pos):
    return post("/selections/definition/primary", {"word_id": word_id, "pos": pos})


def observed_primaries(conn, word_ids):
    """Where each word's primary actually sits right now."""
    found = {}
    ids = list(word_ids)
    for start in range(0, len(ids), 500):
        chunk = ids[start : start + 500]
        marks = ",".join("?" * len(chunk))
        for word_id, pos in conn.execute(
            "SELECT word_id, pos FROM definition_selections"
            f" WHERE is_primary = 1 AND word_id IN ({marks})",
            chunk,
        ):
            found[word_id] = pos
    return found


# ---------------------------------------------------------------------------
# Passes
# ---------------------------------------------------------------------------


def enable_pass(fixes):
    ok = failed = 0
    for fix in fixes:
        if fix["target_enabled"]:
            continue
        if set_enabled(fix["word_id"], fix["to_pos"], True):
            ok += 1
        else:
            failed += 1
            fix["failed"] = True
    print(f"enable: {ok} slots opened, {failed} failed")
    return failed


def converge_pass(fixes):
    """Wait for rule 6, then hand-move whatever it was never going to touch."""
    pending = {f["word_id"]: f for f in fixes if not f.get("failed")}
    manual = [f for f in pending.values() if f["needs_manual_move"]]
    for fix in manual:
        # The reconciler is blind to these: the audit log says a non-reconciler
        # actor moved this primary once, and that is permanent.
        if set_primary(fix["word_id"], fix["to_pos"]):
            fix["moved_manually"] = True
    print(f"converge: {len(manual)} primaries moved by hand (rule 6 cannot see them)")

    deadline = time.time() + CONVERGE_TIMEOUT
    conn = connect()
    while True:
        found = observed_primaries(conn, pending)
        landed = [wid for wid, fix in pending.items() if found.get(wid) == fix["to_pos"]]
        if len(landed) == len(pending) or time.time() >= deadline:
            break
        print(f"converge: {len(landed)}/{len(pending)} primaries in place, waiting…")
        time.sleep(CONVERGE_POLL)
    conn.close()
    for word_id in landed:
        pending[word_id]["converged"] = True
    print(f"converge: {len(landed)}/{len(pending)} primaries in place")
    return len(pending) - len(landed)


def orphan_pass(conn, dry_run=False):
    """Give back a sense to words the triage pass silenced completely.

    The same script that collapsed every word to one slot cut *both* slots on a
    dozen words, so they show no definition at all and their primary sits on a
    disabled row. Re-enabling the slot the primary already occupies changes no
    selection and moves no primary — it only undoes a visibility flip that
    should never have happened. Words whose last slot a person closed are left
    as they are.
    """
    slot_actor = last_slot_actor(conn)
    orphans = conn.execute(
        "SELECT ds.word_id, ds.pos FROM definition_selections ds"
        "  JOIN active_words w ON w.word_id = ds.word_id"
        " WHERE ds.is_primary = 1 AND ds.word_id IN ("
        "   SELECT word_id FROM definition_selections"
        "    GROUP BY word_id HAVING sum(enabled) = 0)"
    ).fetchall()

    ok = held = 0
    for word_id, pos in orphans:
        actor = slot_actor.get((word_id, pos))
        if actor is not None and actor not in MACHINE_ACTORS:
            held += 1
            continue
        if dry_run or set_enabled(word_id, pos, True):
            ok += 1
    verb = "would reopen" if dry_run else "reopened"
    print(f"orphans: {verb} {ok} silenced words, {held} left to their editor")
    return ok


def disable_pass(fixes):
    """Retire the old slots — never the one the primary is standing on."""
    conn = connect()
    converged = [f for f in fixes if f.get("converged")]
    found = observed_primaries(conn, [f["word_id"] for f in converged])
    conn.close()

    ok = skipped = failed = 0
    for fix in converged:
        live = found.get(fix["word_id"])
        if live != fix["to_pos"]:
            # Primary is not where we think; touching a slot now could strand it.
            skipped += 1
            continue
        for pos in fix["retire"]:
            if pos == live:
                skipped += 1
                continue
            if set_enabled(fix["word_id"], pos, False):
                ok += 1
            else:
                failed += 1
    print(f"disable: {ok} slots retired, {skipped} held back, {failed} failed")
    return failed


# ---------------------------------------------------------------------------
# Verification
# ---------------------------------------------------------------------------


def verify(conn, tagged, synsets, spot=()):
    _, skips, disagreeing = plan(conn, tagged, synsets)
    print(f"\nverify: {len(disagreeing)} words still disagree with the corpus")

    orphans = conn.execute(
        "SELECT count(*) FROM ("
        "  SELECT ds.word_id FROM definition_selections ds"
        "  JOIN active_words w ON w.word_id = ds.word_id"
        "  GROUP BY ds.word_id HAVING sum(ds.enabled) = 0)"
    ).fetchone()[0]
    print(f"verify: {orphans} active words with zero enabled slots")

    stranded = conn.execute(
        "SELECT count(*) FROM definition_selections WHERE is_primary = 1 AND enabled = 0"
    ).fetchone()[0]
    print(f"verify: {stranded} primaries sitting on a disabled slot")

    missing = conn.execute(
        "SELECT count(*) FROM ("
        "  SELECT ds.word_id FROM definition_selections ds"
        "  JOIN active_words w ON w.word_id = ds.word_id"
        "  GROUP BY ds.word_id HAVING sum(ds.is_primary) <> 1)"
    ).fetchone()[0]
    print(f"verify: {missing} active words without exactly one primary")

    if spot:
        print("\nspot check:")
        marks = ",".join("?" * len(spot))
        rows = conn.execute(
            "SELECT w.lemma, ds.pos, dc.text"
            "  FROM definition_selections ds"
            "  JOIN words w ON w.word_id = ds.word_id"
            "  JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id"
            f" WHERE ds.is_primary = 1 AND w.lemma IN ({marks})"
            " ORDER BY w.lemma",
            list(spot),
        )
        for lemma, pos, text in rows:
            print(f"  {lemma} ({pos}): {text}")
    return skips


# ---------------------------------------------------------------------------
# Handoff
# ---------------------------------------------------------------------------


def record_changes(conn, path, fixes):
    """Append this run's words to the CLIP-rematch handoff, deduplicated.

    The file is the list of entries the `scorer/2` pass left behind; downstream
    reads it to know which images need re-matching against a changed sense. One
    entry per word, so a word already in there gets its primary refreshed rather
    than a second row.
    """
    path = Path(path)
    if not path.is_file():
        print(f"handoff: {path} missing, not written", file=sys.stderr)
        return
    payload = json.loads(path.read_text())
    if not isinstance(payload, list):
        print(f"handoff: {path} is not a list, not written", file=sys.stderr)
        return
    before = len(payload)

    current = {}
    ids = [fix["word_id"] for fix in fixes]
    for start in range(0, len(ids), 500):
        chunk = ids[start : start + 500]
        marks = ",".join("?" * len(chunk))
        for word_id, pos, text in conn.execute(
            "SELECT ds.word_id, ds.pos, dc.text FROM definition_selections ds"
            "  JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id"
            f" WHERE ds.is_primary = 1 AND ds.word_id IN ({marks})",
            chunk,
        ):
            current[word_id] = (pos, text)

    by_id = {entry["word_id"]: entry for entry in payload}
    added = 0
    for fix in fixes:
        pos, text = current.get(fix["word_id"], (fix["to_pos"], None))
        moved = {"from_pos": fix["from_pos"], "to_pos": pos}
        entry = by_id.get(fix["word_id"])
        if entry is None:
            entry = {
                "word_id": fix["word_id"],
                "lemma": fix["lemma"],
                "primary_moved": moved,
                "definition_changed": [],
                "primary_pos_now": pos,
                "primary_text_now": text,
            }
            payload.append(entry)
            by_id[fix["word_id"]] = entry
            added += 1
            continue
        # Already listed by the scorer pass: keep its record of what changed and
        # bring the primary up to date, so the entry describes the end state.
        origin = (entry.get("primary_moved") or {}).get("from_pos", fix["from_pos"])
        entry["primary_moved"] = {"from_pos": origin, "to_pos": pos}
        entry["primary_pos_now"] = pos
        entry["primary_text_now"] = text

    payload.sort(key=lambda entry: entry["word_id"])
    path.write_text(json.dumps(payload, indent=1) + "\n")
    print(f"handoff: {path.name} {before} -> {len(payload)} entries ({added} new)")


# ---------------------------------------------------------------------------


SPOT = (
    "attorney",
    "allowance",
    "brick",
    "bare",
    "charm",
    "quality",
    "dominant",
    "add",
    "additional",
    "accumulate",
)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dry-run", action="store_true", help="report the plan only")
    ap.add_argument("--verify", action="store_true", help="check the end state only")
    ap.add_argument("--handoff", help="primary_changes.json to append changed ids to")
    args = ap.parse_args()

    tagged, synsets = load_wordnet(WORDNET)
    conn = connect()

    if args.verify:
        verify(conn, tagged, synsets, SPOT)
        return

    fixes, skips, disagreeing = plan(conn, tagged, synsets)
    conn.close()
    print(f"{len(disagreeing)} words disagree with the corpus")
    print(f"  {len(fixes)} fixable, {len(skips)} skipped")
    reasons = defaultdict(int)
    for *_, reason in skips:
        reasons[reason] += 1
    for reason, count in sorted(reasons.items(), key=lambda kv: -kv[1]):
        print(f"    skip: {reason} × {count}")
    blind = sum(1 for f in fixes if f["needs_manual_move"])
    print(f"  {blind} of the fixable are invisible to rule 6 (hand-moved once)")

    if args.dry_run:
        for fix in fixes[:20]:
            print(f"    {fix['lemma']}: {fix['from_pos']} -> {fix['to_pos']}")
        conn = connect()
        orphan_pass(conn, dry_run=True)
        conn.close()
        return

    enable_pass(fixes)
    converge_pass(fixes)
    disable_pass(fixes)
    conn = connect()
    orphan_pass(conn)
    conn.close()

    conn = connect()
    verify(conn, tagged, synsets, SPOT)
    if args.handoff:
        record_changes(conn, args.handoff, [f for f in fixes if f.get("converged")])
    conn.close()


if __name__ == "__main__":
    main()
