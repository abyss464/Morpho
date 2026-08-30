#!/usr/bin/env python3
"""Ingest Visual Genome accepted (image, caption) pairs into Morpho.

For each accepted pair in ops/logs/vg-accepted.json:
  1. Upload the cropped region image as an image candidate.
  2. Mint the VG caption as an example candidate (byte-offset highlight of
     the lemma, found by a whole-word, case-insensitive regex).
  3. Select the uploaded image as the word's live image (human override, pinned).
  4. Select the minted caption into example slot 1.

All mutations go through the admin API (X-Morpho-User: local). Sequential,
one pair at a time. A pair whose caption doesn't contain the lemma as a whole
word, or whose image file is missing, is skipped and logged — never uploaded
without a valid highlight.

Input JSON schema: list of objects with keys:
  word_id, lemma, caption, image_path, region_bbox, vg_image_id, vg_region_id
"""

import json
import os
import re
import unicodedata

import requests

API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012/api")
HEADERS = {"X-Morpho-User": "local"}
INPUT_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "logs", "vg-accepted.json"
)
RESULTS_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "logs", "vg-ingest-results.json"
)


def get_word(word_id):
    r = requests.get(f"{API}/words/{word_id}", timeout=30)
    r.raise_for_status()
    return r.json()


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


def find_highlight(caption, lemma):
    """Find lemma as a whole word in canonicalized text, return UTF-8 byte offsets."""
    text = canonicalize(caption)
    m = re.search(r"\b" + re.escape(lemma) + r"\b", text, re.IGNORECASE)
    if not m:
        return None
    hl_start = len(text[: m.start()].encode("utf-8"))
    hl_end = len(text[: m.end()].encode("utf-8"))
    return hl_start, hl_end


def new_id(field, before_ids, candidates):
    """Find the freshly created candidate id after an upload/mint."""
    ids = [c[field] for c in candidates]
    fresh = [i for i in ids if i not in before_ids]
    return max(fresh) if fresh else (max(ids) if ids else None)


def main():
    pairs = json.load(open(INPUT_PATH))
    total = len(pairs)
    uploaded = 0
    skipped = 0
    errors = []
    no_match = []

    for i, p in enumerate(pairs):
        word_id = p["word_id"]
        lemma = p["lemma"]
        caption = p["caption"]
        image_path = p["image_path"]

        hl = find_highlight(caption, lemma)
        if hl is None:
            skipped += 1
            no_match.append({"word_id": word_id, "lemma": lemma, "caption": caption})
            continue
        hl_start, hl_end = hl

        try:
            with open(image_path, "rb") as f:
                img_bytes = f.read()
        except OSError as e:
            skipped += 1
            errors.append({"word_id": word_id, "stage": "read_image", "error": str(e)})
            continue

        try:
            before = get_word(word_id)
        except requests.RequestException as e:
            skipped += 1
            errors.append({"word_id": word_id, "stage": "get_word", "error": str(e)})
            continue

        before_img_ids = {c["img_cand_id"] for c in before["image"]["candidates"]}
        before_ex_ids = (
            {c["ex_cand_id"] for c in before["examples"][0]["candidates"]}
            if before["examples"]
            else set()
        )

        try:
            # Determine file name and content type from image path
            file_name = os.path.basename(image_path)
            content_type = "image/webp" if file_name.endswith(".webp") else "image/jpeg"

            # 1. Upload image candidate
            r = requests.post(
                f"{API}/candidates/image",
                headers=HEADERS,
                data={"word_id": str(word_id), "source": "vg"},
                files={"file": (file_name, img_bytes, content_type)},
                timeout=60,
            )
            if r.status_code >= 400:
                errors.append(
                    {"word_id": word_id, "stage": "upload_image",
                     "status": r.status_code, "body": r.text[:300]}
                )
                skipped += 1
                continue
            wd = r.json()
            img_cand_id = new_id("img_cand_id", before_img_ids, wd["image"]["candidates"])
            if img_cand_id is None:
                errors.append({"word_id": word_id, "stage": "upload_image",
                                "error": "no candidate found after upload"})
                skipped += 1
                continue

            # 2. Mint example candidate with byte-offset highlight
            r = requests.post(
                f"{API}/candidates/example",
                headers=HEADERS,
                json={"word_id": word_id, "text": caption, "source": "vg",
                      "hl_start": hl_start, "hl_end": hl_end},
                timeout=30,
            )
            if r.status_code >= 400:
                errors.append(
                    {"word_id": word_id, "stage": "mint_example",
                     "status": r.status_code, "body": r.text[:300]}
                )
                skipped += 1
                continue
            wd2 = r.json()
            ex_cands = wd2["examples"][0]["candidates"] if wd2["examples"] else []
            ex_cand_id = new_id("ex_cand_id", before_ex_ids, ex_cands)
            if ex_cand_id is None:
                errors.append({"word_id": word_id, "stage": "mint_example",
                                "error": "no candidate found after mint"})
                skipped += 1
                continue

            # 3. Select the uploaded image
            r = requests.post(
                f"{API}/selections/image",
                headers=HEADERS,
                json={"word_id": word_id, "cand_id": img_cand_id},
                timeout=30,
            )
            if r.status_code >= 400:
                errors.append(
                    {"word_id": word_id, "stage": "select_image",
                     "status": r.status_code, "body": r.text[:300]}
                )
                skipped += 1
                continue

            # 4. Select the minted caption into slot 1
            r = requests.post(
                f"{API}/selections/example",
                headers=HEADERS,
                json={"word_id": word_id, "slot": 1, "cand_id": ex_cand_id},
                timeout=30,
            )
            if r.status_code >= 400:
                errors.append(
                    {"word_id": word_id, "stage": "select_example",
                     "status": r.status_code, "body": r.text[:300]}
                )
                skipped += 1
                continue

            uploaded += 1
        except requests.RequestException as e:
            skipped += 1
            errors.append({"word_id": word_id, "stage": "request_exception", "error": str(e)})
            continue

        if (i + 1) % 50 == 0:
            print(f"Processed {i+1}/{total} (uploaded: {uploaded}, skipped: {skipped})", flush=True)

    summary = {
        "total": total,
        "uploaded": uploaded,
        "skipped": skipped,
        "no_lemma_match": len(no_match),
        "errors": errors,
        "no_lemma_match_details": no_match,
    }
    os.makedirs(os.path.dirname(RESULTS_PATH), exist_ok=True)
    with open(RESULTS_PATH, "w") as f:
        json.dump(summary, f, indent=1)

    print(f"DONE uploaded={uploaded} skipped={skipped} total={total}")
    print(f"results: {RESULTS_PATH}")


if __name__ == "__main__":
    main()
