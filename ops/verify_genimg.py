#!/usr/bin/env python3
"""Upload AI-generated vocabulary images, CLIP-verify each against the
currently selected (incumbent) image, and select the generated image when it
scores higher.

Three phases, run sequentially from __main__:
  1. upload  — POST each PNG to /api/candidates/image (multipart: word_id,
     file). Response is a WordDetail; the new candidate is the max
     img_cand_id under image.candidates for that word_id (freshly minted).
  2. score   — same CLIP model + query-text formula as ops/clip_rematch.py
     (the slot-1 sentence, falling back to the lemma), embed both the new
     candidate's image and the incumbent's image (read straight off the media
     bind mount, no need to re-download), score cosine similarity against the
     query.
  3. select  — where generated > incumbent, POST /api/selections/image (with
     `pin: false`, so a later, better-scoring candidate can still take the
     slot back) then /api/selections/image/approve (mint→select→approve, see
     docs/OPERATIONS.md). Where incumbent wins, leave it untouched.

All mutations go through the admin API; the only direct DB access is a
read-only connection to fetch each word's slot-1 sentence and current
selection (mirrors ops/clip_rematch.py).
"""

import argparse
import json
import os
import sqlite3
import time
import urllib.error
import urllib.request
import uuid

import numpy as np
import open_clip
import torch
from PIL import Image

ROOT = os.environ.get("MORPHO_ROOT", os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DB = f"{ROOT}/data/working.db"
MEDIA = f"{ROOT}/data/media"
API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012/api")

SCRATCH = os.environ.get("MORPHO_GENIMG_DIR", f"{ROOT}/ops/logs/genimg")

# Wave-1 defaults; every one is overridable from the command line (or the
# matching MORPHO_GENIMG_* environment variable) so later waves reuse the
# script unchanged.
DEFAULT_GENIMG_DIR = f"{SCRATCH}/genimg"
DEFAULT_CANDIDATES_JSON = f"{SCRATCH}/image_gen_candidates.json"
DEFAULT_RESULTS_PATH = f"{SCRATCH}/genimg_results.json"
DEFAULT_ACTOR = "operator-genimg"

# Populated by main() from parsed arguments.
GENIMG_DIR = DEFAULT_GENIMG_DIR
CANDIDATES_JSON = DEFAULT_CANDIDATES_JSON
RESULTS_PATH = DEFAULT_RESULTS_PATH
ACTOR = DEFAULT_ACTOR


def parse_args(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument(
        "--input-dir",
        default=os.environ.get("MORPHO_GENIMG_DIR", DEFAULT_GENIMG_DIR),
        help="directory holding {word_id}.png files (default: wave-1 scratchpad)",
    )
    p.add_argument(
        "--words-json",
        default=os.environ.get("MORPHO_GENIMG_WORDS", DEFAULT_CANDIDATES_JSON),
        help="JSON array of objects with at least word_id and lemma",
    )
    p.add_argument(
        "--results",
        default=os.environ.get("MORPHO_GENIMG_RESULTS", ""),
        help="where to write the per-word result JSON "
        "(default: <input-dir>/verify_results.json)",
    )
    p.add_argument(
        "--actor",
        default=os.environ.get("MORPHO_GENIMG_ACTOR", DEFAULT_ACTOR),
        help="value sent as the X-Morpho-User header",
    )
    return p.parse_args(argv)


def api_post_json(path, body):
    req = urllib.request.Request(
        API + path,
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "X-Morpho-User": ACTOR},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.load(resp)


# The engine fits every stored image into a 768x576 box anyway (see
# morpho_reconcile::sources::images::encode), and axum's default per-request
# body limit (2 MiB) sits below several of the source PNGs (up to ~3.7 MB at
# their native 1448x1086). Pre-shrinking to the same target box before upload
# avoids tripping that limit while losing nothing the server would have
# discarded regardless.
UPLOAD_TARGET_BOX = (768, 576)

# Absolute CLIP-score floor: a generated image scoring below this against its
# own query text is treated as a non-match regardless of the incumbent (catches
# off-topic renders that happen to still beat a weak/missing incumbent).
CLIP_FLOOR = 0.08

# Grayscale pixel stddev floor: images at or below this are blank/near-uniform
# (solid white canvas, flat color field) and never worth CLIP-scoring at all.
BLANK_STDDEV_THRESHOLD = 0.5


def is_blank_image(path, threshold=BLANK_STDDEV_THRESHOLD):
    """True if `path` is a blank/near-uniform image: grayscale pixel stddev
    below `threshold`. Catches solid-color renders (e.g. a blank white canvas)
    before they ever reach CLIP scoring or upload."""
    im = Image.open(path).convert("L")
    stddev = float(np.asarray(im, dtype=np.float64).std())
    return stddev < threshold


def _prepare_upload_bytes(file_path):
    im = Image.open(file_path).convert("RGB")
    if im.width > UPLOAD_TARGET_BOX[0] or im.height > UPLOAD_TARGET_BOX[1]:
        im.thumbnail(UPLOAD_TARGET_BOX, Image.LANCZOS)
    import io

    buf = io.BytesIO()
    im.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


def api_upload_image(word_id, file_path):
    boundary = uuid.uuid4().hex
    img_bytes = _prepare_upload_bytes(file_path)
    parts = []
    parts.append(f"--{boundary}\r\n".encode())
    parts.append(
        b'Content-Disposition: form-data; name="word_id"\r\n\r\n' + str(word_id).encode() + b"\r\n"
    )
    parts.append(f"--{boundary}\r\n".encode())
    parts.append(
        f'Content-Disposition: form-data; name="file"; filename="{os.path.basename(file_path)}"\r\n'
        f"Content-Type: image/png\r\n\r\n".encode()
    )
    parts.append(img_bytes)
    parts.append(b"\r\n")
    parts.append(f"--{boundary}--\r\n".encode())
    body = b"".join(parts)

    req = urllib.request.Request(
        API + "/candidates/image",
        data=body,
        headers={
            "Content-Type": f"multipart/form-data; boundary={boundary}",
            "X-Morpho-User": ACTOR,
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.load(resp)


def with_retry(fn, *args, label=""):
    try:
        return True, fn(*args)
    except (urllib.error.HTTPError, urllib.error.URLError) as e:
        print(f"  [retry] {label} failed once: {e}")
        time.sleep(1)
        try:
            return True, fn(*args)
        except (urllib.error.HTTPError, urllib.error.URLError) as e2:
            print(f"  [fail] {label} failed twice: {e2}")
            return False, None


def upload_all(candidates):
    """Phase 1: upload every PNG, return {word_id: {img_cand_id, file_hash}}."""
    uploaded = {}
    failures = []
    for row in candidates:
        wid = row["word_id"]
        png_path = f"{GENIMG_DIR}/{wid}.png"
        ok, detail = with_retry(api_upload_image, wid, png_path, label=f"upload {wid}")
        if not ok or detail is None:
            failures.append({"word_id": wid, "lemma": row["lemma"], "stage": "upload"})
            continue
        cands = detail["image"]["candidates"]
        new_cand = max(cands, key=lambda c: c["img_cand_id"])
        uploaded[wid] = {
            "img_cand_id": new_cand["img_cand_id"],
            "file_hash": new_cand["file_hash"],
        }
        print(f"  uploaded {wid} ({row['lemma']}) -> img_cand_id {new_cand['img_cand_id']}")
    return uploaded, failures


def main(argv=None):
    global GENIMG_DIR, CANDIDATES_JSON, RESULTS_PATH, ACTOR

    args = parse_args(argv)
    GENIMG_DIR = os.path.abspath(args.input_dir)
    CANDIDATES_JSON = args.words_json
    RESULTS_PATH = args.results or f"{GENIMG_DIR}/verify_results.json"
    ACTOR = args.actor

    all_words = json.load(open(CANDIDATES_JSON))
    # Only touch words that actually have a generated PNG waiting; a wave's
    # word list is normally far longer than the batch that has been rendered.
    candidates = [w for w in all_words if os.path.exists(f"{GENIMG_DIR}/{w['word_id']}.png")]
    print(f"Loaded {len(all_words)} words from {CANDIDATES_JSON}; "
          f"{len(candidates)} have a PNG in {GENIMG_DIR}.")
    if not candidates:
        print("Nothing to do.")
        return

    print("\n=== Phase 0: snapshot incumbents ===")
    conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    incumbent_hash = {
        wid: h
        for wid, h in conn.execute(
            """SELECT i.word_id, c.file_hash FROM image_selections i
            JOIN image_candidates c ON c.img_cand_id=i.img_cand_id"""
        )
    }
    incumbent_cid = {wid: cid for wid, cid in conn.execute("SELECT word_id, img_cand_id FROM image_selections")}
    print(f"  snapshotted {len(incumbent_hash)} incumbents BEFORE upload")

    results = []

    print("\n=== Phase 1: blank-image filter (pre-upload) ===")
    surviving = []
    for row in candidates:
        wid = row["word_id"]
        png_path = f"{GENIMG_DIR}/{wid}.png"
        if is_blank_image(png_path):
            print(f"  {wid:>5} {row['lemma']:<16} BLANK IMAGE (stddev < {BLANK_STDDEV_THRESHOLD}) -> skipped, not uploaded")
            results.append({
                "word_id": wid,
                "lemma": row["lemma"],
                "incumbent_score": None,
                "generated_score": None,
                "action": "blank_image",
            })
        else:
            surviving.append(row)
    print(f"  {len(candidates) - len(surviving)} blank image(s) filtered; {len(surviving)} remain")

    print("\n=== Phase 2: CLIP pre-check (floor, pre-upload) ===")
    model, _, preprocess = open_clip.create_model_and_transforms(
        "ViT-B-32", pretrained="laion2b_s34b_b79k"
    )
    tok = open_clip.get_tokenizer("ViT-B-32")
    model = model.cuda().eval()

    def sentence(wid):
        r = conn.execute(
            """SELECT ec.text FROM example_selections es
            JOIN example_candidates ec ON ec.ex_cand_id=es.ex_cand_id
            WHERE es.word_id=? AND es.slot=1""",
            (wid,),
        ).fetchone()
        return r[0] if r else None

    def embed_image_file(path):
        im = preprocess(Image.open(path).convert("RGB")).unsqueeze(0).cuda()
        with torch.no_grad():
            f = model.encode_image(im)
            f = f / f.norm(dim=-1, keepdim=True)
        return f[0]

    def txt_embed(text):
        with torch.no_grad():
            f = model.encode_text(tok([text]).cuda())
            f = f / f.norm(dim=-1, keepdim=True)
        return f[0]

    # Score every surviving candidate against the LOCAL generated PNG (no
    # upload needed yet) plus the incumbent's already-stored image. Anything
    # below the absolute CLIP floor is dropped here, before it ever reaches
    # the upload phase.
    to_upload = []
    scored = {}
    for row in surviving:
        wid = row["word_id"]
        lemma = row["lemma"]

        query = sentence(wid) or lemma
        q = txt_embed(query)

        cur_hash = incumbent_hash.get(wid)
        incumbent_score = None
        if cur_hash is not None:
            try:
                incumbent_score = float(q @ embed_image_file(f"{MEDIA}/{cur_hash[:2]}/{cur_hash}.webp"))
            except FileNotFoundError:
                incumbent_score = None

        try:
            generated_score = float(q @ embed_image_file(f"{GENIMG_DIR}/{wid}.png"))
        except FileNotFoundError:
            generated_score = None

        scored[wid] = (incumbent_score, generated_score)

        if generated_score is None:
            results.append({
                "word_id": wid, "lemma": lemma,
                "incumbent_score": incumbent_score, "generated_score": generated_score,
                "action": "score_failed",
            })
        elif generated_score < CLIP_FLOOR:
            print(
                f"  {wid:>5} {lemma:<16} generated={generated_score:.4f} < floor {CLIP_FLOOR} "
                f"-> below_floor, skipped, not uploaded"
            )
            results.append({
                "word_id": wid, "lemma": lemma,
                "incumbent_score": incumbent_score, "generated_score": generated_score,
                "action": "below_floor",
            })
        else:
            to_upload.append(row)

    print(f"  {len(to_upload)}/{len(surviving)} candidate(s) pass the floor check and will be uploaded")

    print("\n=== Phase 3: upload ===")
    uploaded, upload_failures = upload_all(to_upload)
    print(f"Uploaded {len(uploaded)}/{len(to_upload)}; failures: {len(upload_failures)}")

    print("\n=== Phase 4: decide (generated vs incumbent) ===")
    for row in to_upload:
        wid = row["word_id"]
        lemma = row["lemma"]
        incumbent_score, generated_score = scored[wid]
        entry = {
            "word_id": wid,
            "lemma": lemma,
            "incumbent_score": incumbent_score,
            "generated_score": generated_score,
            "action": None,
        }

        if wid not in uploaded:
            entry["action"] = "upload_failed"
        elif incumbent_score is None or generated_score > incumbent_score:
            entry["action"] = "pending_select"
        else:
            entry["action"] = "kept"

        results.append(entry)
        print(
            f"  {wid:>5} {lemma:<16} incumbent={incumbent_score} generated={generated_score} -> {entry['action']}"
        )

    print("\n=== Phase 5: select winners ===")
    select_failures = []
    for entry in results:
        if entry["action"] != "pending_select":
            continue
        wid = entry["word_id"]
        cand_id = uploaded[wid]["img_cand_id"]
        ok1, _ = with_retry(
            api_post_json,
            "/selections/image",
            {"word_id": wid, "cand_id": cand_id, "pin": False},
            label=f"select {wid}",
        )
        ok2 = False
        if ok1:
            ok2, _ = with_retry(
                api_post_json, "/selections/image/approve", {"word_id": wid}, label=f"approve {wid}"
            )
        if ok1 and ok2:
            entry["action"] = "selected"
            print(f"  selected {wid} ({entry['lemma']})")
        else:
            entry["action"] = "select_failed"
            select_failures.append(entry)

    os.makedirs(os.path.dirname(RESULTS_PATH), exist_ok=True)
    with open(RESULTS_PATH, "w") as f:
        json.dump(results, f, indent=2)

    n_selected = sum(1 for e in results if e["action"] == "selected")
    n_kept = sum(1 for e in results if e["action"] == "kept")
    n_upload_failed = sum(1 for e in results if e["action"] == "upload_failed")
    n_select_failed = sum(1 for e in results if e["action"] == "select_failed")
    n_score_failed = sum(1 for e in results if e["action"] == "score_failed")
    n_blank_image = sum(1 for e in results if e["action"] == "blank_image")
    n_below_floor = sum(1 for e in results if e["action"] == "below_floor")

    print(f"\nDONE selected={n_selected} kept={n_kept} upload_failed={n_upload_failed} "
          f"select_failed={n_select_failed} score_failed={n_score_failed} "
          f"blank_image={n_blank_image} below_floor={n_below_floor}")
    print(f"results: {RESULTS_PATH}")


if __name__ == "__main__":
    main()
