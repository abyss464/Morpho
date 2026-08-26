#!/usr/bin/env python3
"""Upload 48 AI-generated vocabulary images, CLIP-verify each against the
currently selected (incumbent) image, and select the generated image when it
scores higher.

Three phases, run sequentially from __main__:
  1. upload  — POST each PNG to /api/candidates/image (multipart: word_id,
     file). Response is a WordDetail; the new candidate is the max
     img_cand_id under image.candidates for that word_id (freshly minted).
  2. score   — same CLIP model + query-text formula as ops/clip_rematch.py
     (sentence + lemma + trimmed definition), embed both the new candidate's
     image and the incumbent's image (read straight off the media bind mount,
     no need to re-download), score cosine similarity against the query.
  3. select  — where generated > incumbent, POST /api/selections/image then
     /api/selections/image/approve (mint→select→approve, see
     docs/OPERATIONS.md). Where incumbent wins, leave it untouched.

All mutations go through the admin API; the only direct DB access is a
read-only connection to fetch each word's slot-1 sentence, primary
definition, and current selection (mirrors ops/clip_rematch.py).
"""

import json
import os
import sqlite3
import time
import urllib.error
import urllib.request
import uuid

import open_clip
import torch
from PIL import Image

ROOT = "/home/abysser/Code/learning/Morpho"
DB = f"{ROOT}/data/working.db"
MEDIA = f"{ROOT}/data/media"
API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012/api")
ACTOR = "operator-genimg"

SCRATCH = (
    "/tmp/claude-1000/-home-abysser-Code-learning-Morpho/"
    "d3d89616-3276-4389-b525-1b3f57774b29/scratchpad"
)
GENIMG_DIR = f"{SCRATCH}/genimg"
CANDIDATES_JSON = f"{SCRATCH}/image_gen_candidates.json"
RESULTS_PATH = f"{SCRATCH}/genimg_results.json"


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


def main():
    candidates = json.load(open(CANDIDATES_JSON))
    print(f"Loaded {len(candidates)} candidates.")

    print("\n=== Phase 1: upload ===")
    uploaded, upload_failures = upload_all(candidates)
    print(f"Uploaded {len(uploaded)}/{len(candidates)}; failures: {len(upload_failures)}")

    print("\n=== Phase 2: CLIP scoring ===")
    model, _, preprocess = open_clip.create_model_and_transforms(
        "ViT-B-32", pretrained="laion2b_s34b_b79k"
    )
    tok = open_clip.get_tokenizer("ViT-B-32")
    model = model.cuda().eval()

    conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)

    def sentence(wid):
        r = conn.execute(
            """SELECT ec.text FROM example_selections es
            JOIN example_candidates ec ON ec.ex_cand_id=es.ex_cand_id
            WHERE es.word_id=? AND es.slot=1""",
            (wid,),
        ).fetchone()
        return r[0] if r else None

    def definition(wid):
        r = conn.execute(
            """SELECT dc.text FROM definition_selections ds
            JOIN definition_candidates dc ON dc.def_cand_id=ds.def_cand_id
            WHERE ds.word_id=? AND ds.is_primary=1 AND ds.enabled=1""",
            (wid,),
        ).fetchone()
        return r[0] if r else ""

    def img_embed(h):
        im = preprocess(Image.open(f"{MEDIA}/{h[:2]}/{h}.webp").convert("RGB")).unsqueeze(0).cuda()
        with torch.no_grad():
            f = model.encode_image(im)
            f = f / f.norm(dim=-1, keepdim=True)
        return f[0]

    def txt_embed(text):
        with torch.no_grad():
            f = model.encode_text(tok([text]).cuda())
            f = f / f.norm(dim=-1, keepdim=True)
        return f[0]

    incumbent_hash = {
        wid: h
        for wid, h in conn.execute(
            """SELECT i.word_id, c.file_hash FROM image_selections i
            JOIN image_candidates c ON c.img_cand_id=i.img_cand_id"""
        )
    }
    incumbent_cid = {wid: cid for wid, cid in conn.execute("SELECT word_id, img_cand_id FROM image_selections")}

    results = []
    for row in candidates:
        wid = row["word_id"]
        lemma = row["lemma"]
        entry = {
            "word_id": wid,
            "lemma": lemma,
            "incumbent_score": None,
            "generated_score": None,
            "action": None,
        }

        if wid not in uploaded:
            entry["action"] = "upload_failed"
            results.append(entry)
            continue

        s = sentence(wid) or lemma
        query = f"{s} {lemma}: {definition(wid)[:80]}"
        q = txt_embed(query)

        cur_hash = incumbent_hash.get(wid)
        if cur_hash is None:
            entry["action"] = "no_incumbent"
            incumbent_score = None
        else:
            try:
                incumbent_score = float(q @ img_embed(cur_hash))
            except FileNotFoundError:
                incumbent_score = None
        entry["incumbent_score"] = incumbent_score

        gen_hash = uploaded[wid]["file_hash"]
        try:
            generated_score = float(q @ img_embed(gen_hash))
        except FileNotFoundError:
            generated_score = None
        entry["generated_score"] = generated_score

        if generated_score is None:
            entry["action"] = "score_failed"
        elif incumbent_score is None or generated_score > incumbent_score:
            entry["action"] = "pending_select"
        else:
            entry["action"] = "kept"

        results.append(entry)
        print(
            f"  {wid:>5} {lemma:<16} incumbent={incumbent_score} generated={generated_score} -> {entry['action']}"
        )

    print("\n=== Phase 3: select winners ===")
    select_failures = []
    for entry in results:
        if entry["action"] != "pending_select":
            continue
        wid = entry["word_id"]
        cand_id = uploaded[wid]["img_cand_id"]
        ok1, _ = with_retry(
            api_post_json, "/selections/image", {"word_id": wid, "cand_id": cand_id}, label=f"select {wid}"
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

    print(f"\nDONE selected={n_selected} kept={n_kept} upload_failed={n_upload_failed} "
          f"select_failed={n_select_failed} score_failed={n_score_failed}")
    print(f"results: {RESULTS_PATH}")


if __name__ == "__main__":
    main()
