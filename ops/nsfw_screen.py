#!/usr/bin/env python3
"""Screen the image library for inappropriate content (Backlog #9) and
re-gate previously-uploaded candidates against the quality gates (Backlog
B5). Companion to ops/clip_rematch.py and ops/verify_genimg.py — same CLIP
model, same blank/floor formulas, same media-store layout — so scores are
directly comparable across all three scripts.

Subcommands (choose one via the positional `mode` argument):

  regate <candidates.json> --out <results.json> [--apply]
      B5: run the blank-image test and the CLIP floor test against a list of
      {img_cand_id, word_id, lemma, file_hash} objects (query text = the
      word's slot-1 sentence, falling back to the lemma). Writes a
      pass/fail report for every input row. With --apply, also POSTs
      /api/candidates/image/{id}/reject for every failing row.

  clip-sweep --out <report.json> [--limit N]
      Part 2 prong 1: embed every currently SELECTED image once (cached by
      file_hash) and score it against a small battery of inappropriate vs.
      neutral-anchor text probes. Writes a full ranked report; never calls
      the API. Read the top of the report (sorted by margin desc) and
      visually confirm candidates before rejecting anything — this mode
      only flags, it does not judge.

  contact-sheet <word_ids.json> --out-dir <dir> [--per-page N]
      Part 2 prong 2 support: renders every AVAILABLE image candidate for
      the given word_ids into paginated labeled grids (PNG) so a reviewer
      can eyeball many images per Read call instead of one at a time. Each
      thumbnail is labeled "img_cand_id [SEL]" when it is the word's
      current selection.

  reject <to_reject.json> --out <log.json> --actor <name>
      Calls POST /api/candidates/image/{img_cand_id}/reject for every row
      in the input list (must have at least img_cand_id) and logs the
      outcome. The only mode that writes.

Env overrides:
  MORPHO_API    admin API base (default http://127.0.0.1:30012/api)
  MORPHO_DB     path to working.db (default <repo>/data/working.db)
  MORPHO_MEDIA  path to the media store (default <repo>/data/media)
"""

import argparse
import json
import os
import sqlite3
import sys
import urllib.error
import urllib.request

import numpy as np
from PIL import Image, ImageDraw, ImageFont

ROOT = "/home/abysser/Code/learning/Morpho"
DB = os.environ.get("MORPHO_DB", f"{ROOT}/data/working.db")
MEDIA = os.environ.get("MORPHO_MEDIA", f"{ROOT}/data/media")
API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012/api")

# Same constants as ops/verify_genimg.py so B5 re-gating is apples-to-apples
# with the original upload-time gate.
CLIP_FLOOR = 0.08
BLANK_STDDEV_THRESHOLD = 0.5

# Part-2 prong-1 probe battery. "Inappropriate" probes cover the two
# categories called out in the backlog item (sexual content, graphic
# violence); "neutral" anchors represent the bulk of legitimate vocabulary
# imagery (clothed people, objects, nature, diagrams) so a flag requires
# beating the *best* innocent explanation, not just scoring positively on
# its own — CLIP text-image cosine is never near 1.0 even for a clean hit.
INAPPROPRIATE_PROBES = [
    "a nude person",
    "a naked human body",
    "sexually explicit photo",
    "a sexually suggestive photo",
    "pornographic image",
    "close-up photo of genitals",
    "graphic violence, blood and gore",
    "a dead body, corpse",
    "a violent physical assault",
    "a disturbing gory injury",
]
NEUTRAL_PROBES = [
    "a person wearing clothes",
    "a photo of an everyday object or scene",
    "a person doing an everyday activity",
    "a medical illustration or diagram",
    "a landscape or nature photo",
    "a portrait of a person's face",
]

# Near-blank band (owner-added scope, 2026-08-27): stddev < BLANK_STDDEV_THRESHOLD
# (0.5) is the confirmed-blank gate reused from verify_genimg.py; up to this
# higher band is "maybe a solid color with a small logo/watermark" and needs a
# human look rather than an auto-reject (a legitimately low-variance photo —
# clear sky, calm water, snow field — can also land here).
NEAR_BLANK_STDDEV_THRESHOLD = 10.0

# CLIP typographic-bias probes (owner-added scope): CLIP is well known to
# "read" rendered text, so an image whose subject IS text/typography scores
# unusually high against these probes relative to real-scene probes. Used
# only to RANK candidates for a mandatory human look — never to auto-reject,
# since plenty of legitimate images (signs, book covers, charts) also score
# high here and are fine per the "incidental text in a real scene" standard.
TEXT_PROBES = [
    "a word written on a plain background",
    "text, typography, lettering",
    "a screenshot of text",
    "a sign with large text",
]
SCENE_PROBES = [
    "a photograph of a real scene",
    "a natural photo of an object or place",
    "a photo of a person or animal",
]


def media_path(file_hash):
    return f"{MEDIA}/{file_hash[:2]}/{file_hash}.webp"


def is_blank_image(path, threshold=BLANK_STDDEV_THRESHOLD):
    im = Image.open(path).convert("L")
    stddev = float(np.asarray(im, dtype=np.float64).std())
    return stddev < threshold, stddev


def ro_conn():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def sentence_for(conn, word_id):
    r = conn.execute(
        """SELECT ec.text FROM example_selections es
        JOIN example_candidates ec ON ec.ex_cand_id=es.ex_cand_id
        WHERE es.word_id=? AND es.slot=1""",
        (word_id,),
    ).fetchone()
    return r[0] if r else None


def load_clip():
    import open_clip
    import torch

    model, _, preprocess = open_clip.create_model_and_transforms(
        "ViT-B-32", pretrained="laion2b_s34b_b79k"
    )
    tok = open_clip.get_tokenizer("ViT-B-32")
    model = model.cuda().eval()
    return model, tok, preprocess, torch


def api_reject(img_cand_id, actor):
    req = urllib.request.Request(
        f"{API}/candidates/image/{img_cand_id}/reject",
        data=b"{}",
        headers={"Content-Type": "application/json", "X-Morpho-User": actor},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return True, resp.status, None
    except urllib.error.HTTPError as e:
        return False, e.code, e.read().decode(errors="replace")
    except urllib.error.URLError as e:
        return False, None, str(e)


# ---------------------------------------------------------------------------
# regate — B5
# ---------------------------------------------------------------------------

def cmd_regate(args):
    conn = ro_conn()
    candidates = json.load(open(args.candidates))
    model = tok = preprocess = torch = None  # lazy-loaded on first non-blank row

    def txt_embed(text):
        with torch.no_grad():
            f = model.encode_text(tok([text]).cuda())
            f = f / f.norm(dim=-1, keepdim=True)
        return f[0]

    def img_embed(path):
        im = preprocess(Image.open(path).convert("RGB")).unsqueeze(0).cuda()
        with torch.no_grad():
            f = model.encode_image(im)
            f = f / f.norm(dim=-1, keepdim=True)
        return f[0]

    results = []
    for row in candidates:
        wid = row["word_id"]
        lemma = row["lemma"]
        cand_id = row["img_cand_id"]
        file_hash = row["file_hash"]
        path = media_path(file_hash)
        entry = {"img_cand_id": cand_id, "word_id": wid, "lemma": lemma, "file_hash": file_hash}

        if not os.path.exists(path):
            entry.update(gate_failed="missing_file", value=None, pass_=False)
            results.append(entry)
            continue

        blank, stddev = is_blank_image(path)
        if blank:
            entry.update(gate_failed="blank", value=stddev, pass_=False)
            results.append(entry)
            print(f"  FAIL blank   {cand_id:>6} {lemma:<16} stddev={stddev:.4f}")
            continue

        if model is None:
            print("Loading CLIP model...")
            model, tok, preprocess, torch = load_clip()

        query = sentence_for(conn, wid) or lemma
        score = float(txt_embed(query) @ img_embed(path))
        if score < CLIP_FLOOR:
            entry.update(gate_failed="clip_floor", value=score, query=query, pass_=False)
            results.append(entry)
            print(f"  FAIL floor   {cand_id:>6} {lemma:<16} score={score:.4f} query={query!r}")
        else:
            entry.update(gate_failed=None, value=score, query=query, pass_=True)
            results.append(entry)

    n_fail_blank = sum(1 for r in results if r.get("gate_failed") == "blank")
    n_fail_floor = sum(1 for r in results if r.get("gate_failed") == "clip_floor")
    n_fail_missing = sum(1 for r in results if r.get("gate_failed") == "missing_file")
    n_pass = sum(1 for r in results if r.get("pass_"))
    print(
        f"\nDONE: {len(results)} checked, pass={n_pass} "
        f"blank_fail={n_fail_blank} floor_fail={n_fail_floor} missing={n_fail_missing}"
    )

    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(results, f, indent=2)
    print(f"results: {args.out}")

    if args.apply:
        print("\n=== Applying rejections via API ===")
        for r in results:
            if r.get("pass_"):
                continue
            ok, status, err = api_reject(r["img_cand_id"], args.actor)
            r["rejected"] = ok
            r["reject_status"] = status
            if not ok:
                r["reject_error"] = err
            print(f"  reject {r['img_cand_id']:>6} {r['lemma']:<16} -> {'OK' if ok else 'FAILED ' + str(err)}")
        with open(args.out, "w") as f:
            json.dump(results, f, indent=2)


# ---------------------------------------------------------------------------
# blank-scan — owner-added scope: solid/near-solid color images, library-wide
# ---------------------------------------------------------------------------

def cmd_blank_scan(args):
    conn = ro_conn()
    rows = conn.execute(
        """SELECT s.word_id, w.lemma, s.img_cand_id, c.file_hash
        FROM image_selections s
        JOIN words w ON w.word_id = s.word_id
        JOIN image_candidates c ON c.img_cand_id = s.img_cand_id
        WHERE c.status = 'available'
        ORDER BY s.word_id"""
    ).fetchall()
    if args.limit:
        rows = rows[: args.limit]
    print(f"Scanning {len(rows)} selected available images for blank/near-blank...")

    hash_stddev = {}
    report = []
    for i, (wid, lemma, cand_id, file_hash) in enumerate(rows):
        if file_hash not in hash_stddev:
            try:
                _, stddev = is_blank_image(media_path(file_hash))
                hash_stddev[file_hash] = stddev
            except Exception as e:
                hash_stddev[file_hash] = None
                print(f"  [warn] failed for {file_hash}: {e}")
        stddev = hash_stddev[file_hash]
        if stddev is None:
            continue
        if stddev < BLANK_STDDEV_THRESHOLD:
            klass = "confirmed_blank"
        elif stddev < NEAR_BLANK_STDDEV_THRESHOLD:
            klass = "near_blank"
        else:
            klass = "fine"
        report.append({
            "word_id": wid, "lemma": lemma, "img_cand_id": cand_id, "file_hash": file_hash,
            "stddev": stddev, "class": klass,
        })
        if (i + 1) % 1000 == 0:
            print(f"  {i+1}/{len(rows)}", flush=True)

    report.sort(key=lambda r: r["stddev"])
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(report, f, indent=1)

    n_confirmed = sum(1 for r in report if r["class"] == "confirmed_blank")
    n_near = sum(1 for r in report if r["class"] == "near_blank")
    print(f"DONE. confirmed_blank={n_confirmed} near_blank={n_near} fine={len(report)-n_confirmed-n_near}")
    print(f"report: {args.out}")


# ---------------------------------------------------------------------------
# text-scan — owner-added scope: text-dominant images, library-wide
# ---------------------------------------------------------------------------

def cmd_text_scan(args):
    conn = ro_conn()
    rows = conn.execute(
        """SELECT s.word_id, w.lemma, s.img_cand_id, c.file_hash
        FROM image_selections s
        JOIN words w ON w.word_id = s.word_id
        JOIN image_candidates c ON c.img_cand_id = s.img_cand_id
        WHERE c.status = 'available'
        ORDER BY s.word_id"""
    ).fetchall()
    if args.limit:
        rows = rows[: args.limit]
    print(f"Scanning {len(rows)} selected available images for text-dominance...")

    model, tok, preprocess, torch = load_clip()

    def txt_embed(text):
        with torch.no_grad():
            f = model.encode_text(tok([text]).cuda())
            f = f / f.norm(dim=-1, keepdim=True)
        return f[0]

    def img_embed(path):
        im = preprocess(Image.open(path).convert("RGB")).unsqueeze(0).cuda()
        with torch.no_grad():
            f = model.encode_image(im)
            f = f / f.norm(dim=-1, keepdim=True)
        return f[0]

    text_embeds = [(p, txt_embed(p)) for p in TEXT_PROBES]
    scene_embeds = [(p, txt_embed(p)) for p in SCENE_PROBES]

    hash_cache = {}
    report = []
    for i, (wid, lemma, cand_id, file_hash) in enumerate(rows):
        if file_hash not in hash_cache:
            try:
                hash_cache[file_hash] = img_embed(media_path(file_hash))
            except Exception as e:
                hash_cache[file_hash] = None
                print(f"  [warn] embed failed for {file_hash}: {e}")
        e = hash_cache[file_hash]
        if e is None:
            continue
        best_text_probe, best_text_score = max(
            ((p, float(e @ v)) for p, v in text_embeds), key=lambda t: t[1]
        )
        best_scene_probe, best_scene_score = max(
            ((p, float(e @ v)) for p, v in scene_embeds), key=lambda t: t[1]
        )
        margin = best_text_score - best_scene_score
        report.append({
            "word_id": wid, "lemma": lemma, "img_cand_id": cand_id, "file_hash": file_hash,
            "best_text_probe": best_text_probe, "best_text_score": best_text_score,
            "best_scene_probe": best_scene_probe, "best_scene_score": best_scene_score,
            "margin": margin,
        })
        if (i + 1) % 1000 == 0:
            print(f"  {i+1}/{len(rows)}", flush=True)

    report.sort(key=lambda r: r["margin"], reverse=True)
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(report, f, indent=1)
    print(f"DONE. report: {args.out} ({len(report)} rows, sorted by margin desc)")
    print("NOTE: this ranks candidates only — every flag needs a human Read-tool look "
          "before rejecting (mandated: CLIP text-probe bias is not sufficient evidence alone).")


# ---------------------------------------------------------------------------
# clip-sweep — Part 2 prong 1
# ---------------------------------------------------------------------------

def cmd_clip_sweep(args):
    conn = ro_conn()
    rows = conn.execute(
        """SELECT s.word_id, w.lemma, s.img_cand_id, c.file_hash
        FROM image_selections s
        JOIN words w ON w.word_id = s.word_id
        JOIN image_candidates c ON c.img_cand_id = s.img_cand_id
        ORDER BY s.word_id"""
    ).fetchall()
    if args.limit:
        rows = rows[: args.limit]
    print(f"Sweeping {len(rows)} selected images ({len({h for *_, h in rows})} distinct hashes)...")

    model, tok, preprocess, torch = load_clip()

    def txt_embed(text):
        with torch.no_grad():
            f = model.encode_text(tok([text]).cuda())
            f = f / f.norm(dim=-1, keepdim=True)
        return f[0]

    def img_embed(path):
        im = preprocess(Image.open(path).convert("RGB")).unsqueeze(0).cuda()
        with torch.no_grad():
            f = model.encode_image(im)
            f = f / f.norm(dim=-1, keepdim=True)
        return f[0]

    bad_embeds = [(p, txt_embed(p)) for p in INAPPROPRIATE_PROBES]
    neutral_embeds = [(p, txt_embed(p)) for p in NEUTRAL_PROBES]

    hash_cache = {}
    report = []
    for i, (wid, lemma, cand_id, file_hash) in enumerate(rows):
        if file_hash not in hash_cache:
            path = media_path(file_hash)
            try:
                hash_cache[file_hash] = img_embed(path)
            except Exception as e:
                hash_cache[file_hash] = None
                print(f"  [warn] embed failed for {file_hash}: {e}")
        e = hash_cache[file_hash]
        if e is None:
            report.append({
                "word_id": wid, "lemma": lemma, "img_cand_id": cand_id, "file_hash": file_hash,
                "error": "embed_failed",
            })
            continue

        best_bad_probe, best_bad_score = max(
            ((p, float(e @ v)) for p, v in bad_embeds), key=lambda t: t[1]
        )
        best_neutral_probe, best_neutral_score = max(
            ((p, float(e @ v)) for p, v in neutral_embeds), key=lambda t: t[1]
        )
        margin = best_bad_score - best_neutral_score
        report.append({
            "word_id": wid, "lemma": lemma, "img_cand_id": cand_id, "file_hash": file_hash,
            "best_bad_probe": best_bad_probe, "best_bad_score": best_bad_score,
            "best_neutral_probe": best_neutral_probe, "best_neutral_score": best_neutral_score,
            "margin": margin,
        })

        if (i + 1) % 1000 == 0:
            print(f"  {i+1}/{len(rows)}", flush=True)

    report.sort(key=lambda r: r.get("margin", -99), reverse=True)
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(report, f, indent=1)
    print(f"DONE. report: {args.out} ({len(report)} rows, sorted by margin desc)")
    print("Top 15:")
    for r in report[:15]:
        if "error" in r:
            continue
        print(
            f"  margin={r['margin']:+.4f} bad={r['best_bad_score']:.4f} "
            f"({r['best_bad_probe']!r}) neutral={r['best_neutral_score']:.4f} "
            f"word={r['lemma']} img_cand_id={r['img_cand_id']}"
        )


# ---------------------------------------------------------------------------
# contact-sheet — Part 2 prong 2 support
# ---------------------------------------------------------------------------

THUMB = 200
LABEL_H = 22
HEADER_H = 22
PAD = 8


def _font(size):
    try:
        return ImageFont.load_default(size=size)
    except TypeError:
        return ImageFont.load_default()


def cmd_flagged_sheet(args):
    """Render one thumbnail per entry from a clip-sweep report (already
    filtered to the rows worth a human look), for a first-pass "is this
    specific selected image inappropriate" judgment — as opposed to
    contact-sheet, which shows every available candidate for a word."""
    entries = json.load(open(args.entries))
    os.makedirs(args.out_dir, exist_ok=True)

    cols = args.cols
    thumb = args.thumb
    cell_w = thumb + PAD
    cell_h = thumb + LABEL_H * 2 + PAD
    per_page = cols * args.rows

    lbl_font = _font(13)

    pages = [entries[i : i + per_page] for i in range(0, len(entries), per_page)]
    written = []
    for pi, page_entries in enumerate(pages):
        rows_needed = -(-len(page_entries) // cols)
        img = Image.new("RGB", (cols * cell_w + PAD, rows_needed * cell_h + PAD), "white")
        draw = ImageDraw.Draw(img)
        for i, e in enumerate(page_entries):
            col, row = i % cols, i // cols
            x = PAD + col * cell_w
            y = PAD + row * cell_h
            try:
                th = Image.open(media_path(e["file_hash"])).convert("RGB")
                th.thumbnail((thumb, thumb), Image.LANCZOS)
            except Exception:
                th = Image.new("RGB", (thumb, thumb), "gray")
            tx = x + (thumb - th.width) // 2
            ty = y + (thumb - th.height) // 2
            img.paste(th, (tx, ty))
            draw.rectangle([x, y, x + thumb, y + thumb], outline="red", width=2)
            l1 = f"{e.get('img_cand_id')} {e.get('lemma','')}"
            l2 = f"m={e.get('margin', 0):+.3f} {e.get('best_bad_probe','')[:22]}"
            draw.text((x, y + thumb + 2), l1, fill="black", font=lbl_font)
            draw.text((x, y + thumb + 2 + LABEL_H), l2, fill="darkred", font=lbl_font)
        out_path = f"{args.out_dir}/flagged_{pi:02d}.png"
        img.save(out_path)
        written.append(out_path)
        print(f"  wrote {out_path} ({len(page_entries)} images)")

    print(f"DONE. {len(written)} page(s), {len(entries)} images total, in {args.out_dir}")


def cmd_contact_sheet(args):
    conn = ro_conn()
    word_ids = json.load(open(args.word_ids))
    os.makedirs(args.out_dir, exist_ok=True)

    words = []
    for wid in word_ids:
        lemma_row = conn.execute("SELECT lemma FROM words WHERE word_id=?", (wid,)).fetchone()
        lemma = lemma_row[0] if lemma_row else f"word{wid}"
        cands = conn.execute(
            """SELECT img_cand_id, file_hash FROM image_candidates
            WHERE word_id=? AND status='available' ORDER BY img_cand_id""",
            (wid,),
        ).fetchall()
        sel = conn.execute("SELECT img_cand_id FROM image_selections WHERE word_id=?", (wid,)).fetchone()
        sel_cid = sel[0] if sel else None
        words.append((wid, lemma, cands, sel_cid))

    max_cols = max((len(c) for _, _, c, _ in words), default=1) or 1
    row_h = HEADER_H + THUMB + LABEL_H + PAD
    page_w = max_cols * THUMB + PAD
    per_page = args.per_page

    hdr_font = _font(16)
    lbl_font = _font(13)

    pages = [words[i : i + per_page] for i in range(0, len(words), per_page)]
    written = []
    for pi, page_words in enumerate(pages):
        img = Image.new("RGB", (page_w, row_h * len(page_words) + PAD), "white")
        draw = ImageDraw.Draw(img)
        y = PAD
        for wid, lemma, cands, sel_cid in page_words:
            draw.text((PAD, y), f"{wid} {lemma}  ({len(cands)} available)", fill="black", font=hdr_font)
            x = PAD
            for cand_id, file_hash in cands:
                try:
                    thumb = Image.open(media_path(file_hash)).convert("RGB")
                    thumb.thumbnail((THUMB, THUMB), Image.LANCZOS)
                except Exception:
                    thumb = Image.new("RGB", (THUMB, THUMB), "gray")
                tx = x + (THUMB - thumb.width) // 2
                ty = y + HEADER_H + (THUMB - thumb.height) // 2
                img.paste(thumb, (tx, ty))
                draw.rectangle([x, y + HEADER_H, x + THUMB, y + HEADER_H + THUMB], outline="black")
                label = f"{cand_id}" + (" [SEL]" if cand_id == sel_cid else "")
                color = "red" if cand_id == sel_cid else "black"
                draw.text((x + 2, y + HEADER_H + THUMB + 2), label, fill=color, font=lbl_font)
                x += THUMB
            y += row_h
        out_path = f"{args.out_dir}/sheet_{pi:02d}.png"
        img.save(out_path)
        written.append(out_path)
        print(f"  wrote {out_path} ({len(page_words)} words)")

    manifest_path = f"{args.out_dir}/manifest.json"
    with open(manifest_path, "w") as f:
        json.dump(
            [{"word_id": w, "lemma": l, "candidates": [c for c, _ in cands], "selected": s}
             for w, l, cands, s in words],
            f, indent=1,
        )
    print(f"DONE. {len(written)} page(s) in {args.out_dir}; manifest: {manifest_path}")


# ---------------------------------------------------------------------------
# reject — the only writing mode
# ---------------------------------------------------------------------------

def cmd_reject(args):
    to_reject = json.load(open(args.to_reject))
    results = []
    for row in to_reject:
        cand_id = row["img_cand_id"]
        ok, status, err = api_reject(cand_id, args.actor)
        entry = dict(row)
        entry["rejected"] = ok
        entry["reject_status"] = status
        if not ok:
            entry["reject_error"] = err
        results.append(entry)
        print(f"  reject {cand_id:>6} word={row.get('word_id')} lemma={row.get('lemma')!s:<16} -> {'OK' if ok else 'FAILED ' + str(err)}")

    n_ok = sum(1 for r in results if r["rejected"])
    print(f"\nDONE: {n_ok}/{len(results)} rejected successfully")
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(results, f, indent=2)
    print(f"log: {args.out}")


# ---------------------------------------------------------------------------

def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest="mode", required=True)

    p_regate = sub.add_parser("regate", help="B5: blank + CLIP-floor gate re-check")
    p_regate.add_argument("candidates", help="JSON list of {img_cand_id, word_id, lemma, file_hash}")
    p_regate.add_argument("--out", required=True)
    p_regate.add_argument("--apply", action="store_true", help="also POST /reject for failing rows")
    p_regate.add_argument("--actor", default="operator-imgclean")
    p_regate.set_defaults(func=cmd_regate)

    p_sweep = sub.add_parser("clip-sweep", help="Part 2 prong 1: NSFW probe screen over all selected images")
    p_sweep.add_argument("--out", required=True)
    p_sweep.add_argument("--limit", type=int, default=0)
    p_sweep.set_defaults(func=cmd_clip_sweep)

    p_blank = sub.add_parser("blank-scan", help="solid/near-solid color scan over all selected images")
    p_blank.add_argument("--out", required=True)
    p_blank.add_argument("--limit", type=int, default=0)
    p_blank.set_defaults(func=cmd_blank_scan)

    p_text = sub.add_parser("text-scan", help="text-dominant image scan over all selected images (ranks only)")
    p_text.add_argument("--out", required=True)
    p_text.add_argument("--limit", type=int, default=0)
    p_text.set_defaults(func=cmd_text_scan)

    p_sheet = sub.add_parser("contact-sheet", help="Part 2 prong 2: labeled image grids for visual review")
    p_sheet.add_argument("word_ids", help="JSON list of word_id ints")
    p_sheet.add_argument("--out-dir", required=True)
    p_sheet.add_argument("--per-page", type=int, default=8)
    p_sheet.set_defaults(func=cmd_contact_sheet)

    p_flagged = sub.add_parser("flagged-sheet", help="grid of single flagged images from a clip-sweep report")
    p_flagged.add_argument("entries", help="JSON list of clip-sweep entries (img_cand_id, file_hash, lemma, margin, best_bad_probe, ...)")
    p_flagged.add_argument("--out-dir", required=True)
    p_flagged.add_argument("--cols", type=int, default=6)
    p_flagged.add_argument("--rows", type=int, default=6)
    p_flagged.add_argument("--thumb", type=int, default=220)
    p_flagged.set_defaults(func=cmd_flagged_sheet)

    p_reject = sub.add_parser("reject", help="Apply rejections via the admin API")
    p_reject.add_argument("to_reject", help="JSON list of {img_cand_id, ...}")
    p_reject.add_argument("--out", required=True)
    p_reject.add_argument("--actor", default="operator-imgclean")
    p_reject.set_defaults(func=cmd_reject)

    args = p.parse_args(argv)
    args.func(args)


if __name__ == "__main__":
    main()
