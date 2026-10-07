#!/usr/bin/env python3
"""Prepare Wikipedia Image Text (WIT) pairs for Morpho ingest.

Takes:
  - /tmp/wit_gap_matches.json (from WIT coverage scan)
  - /tmp/stock_only_words.tsv (for current word_ids)

Outputs:
  - ops/logs/wit-accepted.json (input for wit_ingest.py)
  - Downloaded images in /tmp/wit/webp/

WIT images are from Wikimedia Commons — more stable URLs than CC3M.
Captions are attribution descriptions from Wikipedia articles.
"""

import json
import os
import re
import hashlib
from concurrent.futures import ThreadPoolExecutor, as_completed
from io import BytesIO

import requests

try:
    from PIL import Image
except ImportError:
    import sys
    sys.exit("Pillow is required: pip install Pillow")


MATCHES_PATH = "/tmp/wit_gap_matches.json"
STOCK_WORDS_PATH = "/tmp/stock_only_words.tsv"
WEBP_DIR = "/tmp/wit/webp"
ACCEPTED_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "logs", "wit-accepted.json"
)
WEBP_QUALITY = 80
DOWNLOAD_TIMEOUT = 30
MAX_WORKERS = 3
MIN_IMAGE_DIM = 128
REQUEST_DELAY = 1.0  # seconds between requests to avoid 429


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


def score_caption(caption, lemma):
    """Score caption quality for vocabulary illustration."""
    words = caption.split()
    wc = len(words)
    score = 0.0

    if wc < 4:
        return -10.0
    if 5 <= wc <= 20:
        score += 3.0
    elif 4 <= wc <= 30:
        score += 1.5

    # Penalize non-photographs (WIT has many flags, maps, logos)
    non_photo = re.search(
        r"\b(flag|coat of arms|map|logo|diagram|chart|graph|emblem|seal|"
        r"icon|shield|insignia|crest|banner|medal|stamp|coin|"
        r"vector|illustration|clipart|cartoon|drawing|sketch)\b",
        caption, re.IGNORECASE,
    )
    if non_photo:
        score -= 5.0

    # Bonus for photo-like descriptions
    if re.search(r"\b(photo|photograph|view|scene|building|street|"
                 r"person|people|landscape|city|village)\b", caption, re.IGNORECASE):
        score += 1.0

    return score


def download_image(url, timeout=DOWNLOAD_TIMEOUT):
    try:
        import time
        time.sleep(REQUEST_DELAY)
        headers = {
            "User-Agent": "Morpho-vocab-research/1.0 (educational vocabulary app"
            + (f"; {os.environ['MORPHO_CONTACT']}" if os.environ.get("MORPHO_CONTACT") else "") + ")",
            "Accept": "image/*",
        }
        for attempt in range(3):
            r = requests.get(url, headers=headers, timeout=timeout, stream=True)
            if r.status_code == 429:
                retry_after = int(r.headers.get("Retry-After", 5))
                time.sleep(max(retry_after, 3))
                continue
            break
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
    """Download best image for a word from WIT matches."""
    matches = word_data["matches"]
    scored = [(score_caption(m["caption"], lemma), m) for m in matches]
    scored.sort(key=lambda x: x[0], reverse=True)

    for sc, m in scored:
        if sc < -3:
            continue
        url = m["image_url"]
        if not url:
            continue
        caption = m["caption"]
        matched_form = m.get("matched_form", lemma)

        img_bytes = download_image(url)
        if img_bytes is None:
            continue

        url_hash = hashlib.md5(url.encode()).hexdigest()[:12]
        webp_name = f"wit_{word_data['word_id']}_{url_hash}.webp"
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

    all_matches = json.load(open(MATCHES_PATH))
    print(f"WIT matched lemmas: {len(all_matches)}", flush=True)

    # Filter to current stock-only words, exclude words already covered
    cc3m_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "logs", "cc3m-accepted.json")
    cc3m_infl_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "logs", "cc3m-inflected-accepted.json")

    covered = set()
    for path in [cc3m_path, cc3m_infl_path]:
        if os.path.isfile(path):
            for p in json.load(open(path)):
                covered.add(p["lemma"])

    to_process = {}
    for lemma, data in all_matches.items():
        if lemma in stock_words and lemma not in covered:
            data["word_id"] = stock_words[lemma]
            to_process[lemma] = data

    print(f"To process (stock-only, not yet covered): {len(to_process)}", flush=True)

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
    print(f"WIT PREPARE RESULTS")
    print(f"{'=' * 60}")
    print(f"Words to process: {len(to_process)}")
    print(f"Successfully prepared: {len(accepted)}")
    print(f"Failed (dead URLs / bad images): {failed}")
    print(f"Accepted pairs written to: {ACCEPTED_PATH}")
    print(f"{'=' * 60}")


if __name__ == "__main__":
    main()
