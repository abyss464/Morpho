#!/usr/bin/env python3
"""Prepare CC3M inflected-match pairs for Morpho ingest.

Like cc3m_prepare.py but matches inflected forms of lemmas
(e.g., "abolished" for lemma "abolish"). The highlight in the
ingested example will mark the inflected form, not the bare lemma.

Takes:
  - /tmp/cc3m_inflected_matches.json (from inflected coverage scan)
  - /tmp/stock_only_words.tsv (for current word_ids)

Outputs:
  - ops/logs/cc3m-inflected-accepted.json
  - Downloaded images in /tmp/cc3m_inflected/webp/
"""

import json
import os
import re
import hashlib
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed
from io import BytesIO

import requests

try:
    from PIL import Image
except ImportError:
    import sys
    sys.exit("Pillow is required: pip install Pillow")


MATCHES_PATH = "/tmp/cc3m_inflected_matches.json"
STOCK_WORDS_PATH = "/tmp/stock_only_words.tsv"
WEBP_DIR = "/tmp/cc3m_inflected/webp"
ACCEPTED_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "logs", "cc3m-inflected-accepted.json"
)
WEBP_QUALITY = 80
DOWNLOAD_TIMEOUT = 15
MAX_WORKERS = 16
MIN_IMAGE_DIM = 128


def load_stock_words():
    words = {}
    with open(STOCK_WORDS_PATH) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            parts = line.split("|")
            if len(parts) >= 2:
                words[parts[1].lower()] = int(parts[0])
    return words


def score_caption(caption, lemma, matched_form):
    """Score caption quality, with photo-vs-illustration filtering."""
    words = caption.split()
    wc = len(words)
    score = 0.0

    if wc < 4:
        return -10.0
    if 5 <= wc <= 15:
        score += 3.0
    elif 4 <= wc <= 20:
        score += 1.5

    # Penalize non-photographs
    non_photo = re.search(
        r"\b(vector|illustration|clipart|icon|logo|cartoon|drawing|sketch|"
        r"infographic|diagram|graphic|template|render|3d)\b",
        caption, re.IGNORECASE,
    )
    if non_photo:
        score -= 5.0

    # Bonus for concrete descriptions
    if re.match(r"^(a|an|the|this|that|some)\b", caption, re.IGNORECASE):
        score += 0.5

    return score


def download_image(url, timeout=DOWNLOAD_TIMEOUT):
    try:
        headers = {
            "User-Agent": "Mozilla/5.0 (compatible; research-bot/1.0)",
            "Accept": "image/*",
        }
        r = requests.get(url, headers=headers, timeout=timeout, stream=True)
        if r.status_code != 200:
            return None
        content_type = r.headers.get("Content-Type", "")
        if content_type and not content_type.startswith("image/"):
            return None
        data = b""
        for chunk in r.iter_content(8192):
            data += chunk
            if len(data) > 10 * 1024 * 1024:
                return None
        if len(data) < 1000:
            return None
        return data
    except Exception:
        return None


def convert_to_webp(img_bytes, output_path):
    try:
        img = Image.open(BytesIO(img_bytes))
        if img.mode in ("RGBA", "P"):
            img = img.convert("RGB")
        w, h = img.size
        if w < MIN_IMAGE_DIM or h < MIN_IMAGE_DIM:
            return False
        max_dim = max(w, h)
        if max_dim > 512:
            scale = 512 / max_dim
            img = img.resize((int(w * scale), int(h * scale)), Image.LANCZOS)
        img.save(output_path, "WEBP", quality=WEBP_QUALITY)
        return True
    except Exception:
        return False


def process_word(lemma, word_data):
    """Download best image for a word matched via inflected form."""
    matches = word_data["matches"]
    scored = [(score_caption(m["caption"], lemma, m.get("matched_form", "")), m) for m in matches]
    scored.sort(key=lambda x: x[0], reverse=True)

    for sc, m in scored:
        if sc < -3:
            continue
        url = m["image_url"]
        caption = m["caption"]
        matched_form = m.get("matched_form", lemma)

        img_bytes = download_image(url)
        if img_bytes is None:
            continue

        url_hash = hashlib.md5(url.encode()).hexdigest()[:12]
        webp_name = f"cc3mi_{word_data['word_id']}_{url_hash}.webp"
        webp_path = os.path.join(WEBP_DIR, webp_name)
        if not convert_to_webp(img_bytes, webp_path):
            continue

        return {
            "word_id": word_data["word_id"],
            "lemma": lemma,
            "caption": caption,
            "matched_form": matched_form,
            "image_path": webp_path,
            "file_name": webp_name,
            "source_url": url,
        }

    return None


def main():
    stock_words = load_stock_words()
    print(f"Current stock-only words: {len(stock_words)}", flush=True)

    # Re-check what's still stock-only after previous ingests
    # (the stock_only_words.tsv was last exported before CC3M ingest)
    # We filter to words that are actually in the current stock-only set
    all_matches = json.load(open(MATCHES_PATH))
    print(f"Inflected-matched lemmas (total): {len(all_matches)}", flush=True)

    # Also exclude words already covered by CC3M ingest
    cc3m_accepted_path = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "logs", "cc3m-accepted.json"
    )
    cc3m_covered = set()
    if os.path.isfile(cc3m_accepted_path):
        cc3m_accepted = json.load(open(cc3m_accepted_path))
        cc3m_covered = {p["lemma"] for p in cc3m_accepted}

    to_process = {}
    for lemma, data in all_matches.items():
        if lemma in stock_words and lemma not in cc3m_covered:
            data["word_id"] = stock_words[lemma]
            to_process[lemma] = data

    print(f"To process (stock-only, not CC3M-covered): {len(to_process)}", flush=True)

    os.makedirs(WEBP_DIR, exist_ok=True)
    os.makedirs(os.path.dirname(ACCEPTED_PATH), exist_ok=True)

    accepted = []
    failed = 0
    processed = 0

    with ThreadPoolExecutor(max_workers=MAX_WORKERS) as pool:
        futures = {
            pool.submit(process_word, lemma, data): lemma
            for lemma, data in to_process.items()
        }

        for future in as_completed(futures):
            lemma = futures[future]
            processed += 1
            try:
                result = future.result()
                if result:
                    accepted.append(result)
                else:
                    failed += 1
            except Exception:
                failed += 1

            if processed % 50 == 0:
                print(
                    f"  {processed}/{len(to_process)} processed, "
                    f"{len(accepted)} accepted, {failed} failed",
                    flush=True,
                )

    accepted.sort(key=lambda x: x["word_id"])

    with open(ACCEPTED_PATH, "w") as f:
        json.dump(accepted, f, indent=1)

    print(f"\n{'=' * 60}")
    print(f"CC3M INFLECTED PREPARE RESULTS")
    print(f"{'=' * 60}")
    print(f"Words to process: {len(to_process)}")
    print(f"Successfully prepared: {len(accepted)}")
    print(f"Failed (dead URLs / bad images): {failed}")
    print(f"Accepted pairs written to: {ACCEPTED_PATH}")
    print(f"{'=' * 60}")


if __name__ == "__main__":
    main()
