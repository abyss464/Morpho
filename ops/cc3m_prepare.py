#!/usr/bin/env python3
"""Prepare Conceptual Captions 3M pairs for Morpho ingest.

Takes:
  - /tmp/cc3m_matches.json (from coverage analysis: lemma -> {word_id, matches})
  - /tmp/stock_only_words.tsv (to filter to current stock-only words)

Outputs:
  - ops/logs/cc3m-accepted.json (input for cc3m_ingest.py)
  - Downloaded images in /tmp/cc3m/images/

For each stock-only word with CC3M matches, downloads images from URLs
(handling dead links), converts to WebP, picks the best pair by caption
quality, and outputs the accepted list.
"""

import json
import os
import re
import sys
import time
import hashlib
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed
from io import BytesIO
from pathlib import Path
from urllib.parse import urlparse

import requests

try:
    from PIL import Image
except ImportError:
    sys.exit("Pillow is required: pip install Pillow")


MATCHES_PATH = "/tmp/cc3m_matches.json"
STOCK_WORDS_PATH = "/tmp/stock_only_words.tsv"
IMAGES_DIR = "/tmp/cc3m/images"
WEBP_DIR = "/tmp/cc3m/webp"
ACCEPTED_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "logs", "cc3m-accepted.json"
)
WEBP_QUALITY = 80
DOWNLOAD_TIMEOUT = 15
MAX_WORKERS = 16
MIN_IMAGE_DIM = 128


def load_stock_words():
    """Load current stock-only words."""
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

    # Sweet spot: 5-15 words
    if 5 <= wc <= 15:
        score += 3.0
    elif 4 <= wc <= 20:
        score += 1.5
    elif wc < 4:
        return -10.0  # too short

    # Penalize if lemma is the only content word
    if caption.lower().strip() == lemma:
        score -= 5.0

    # Bonus for sentence-like captions (starts with article/demonstrative)
    if re.match(r"^(a|an|the|this|that|some)\b", caption, re.IGNORECASE):
        score += 0.5

    # Penalize vector/illustration/clipart mentions (not photographs)
    non_photo = re.search(
        r"\b(vector|illustration|clipart|icon|logo|cartoon|drawing|sketch|"
        r"infographic|diagram|graphic|template|render|3d)\b",
        caption, re.IGNORECASE,
    )
    if non_photo:
        score -= 5.0

    # Bonus for concrete, descriptive language
    if re.search(r"\b(photo|photograph|image|picture|view|scene)\b", caption, re.IGNORECASE):
        score += 0.5

    return score


def download_image(url, timeout=DOWNLOAD_TIMEOUT):
    """Download an image from URL, return bytes or None."""
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
        # Read up to 10MB
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
    """Convert image bytes to WebP, return True on success."""
    try:
        img = Image.open(BytesIO(img_bytes))
        if img.mode in ("RGBA", "P"):
            img = img.convert("RGB")
        w, h = img.size
        if w < MIN_IMAGE_DIM or h < MIN_IMAGE_DIM:
            return False
        # Resize if very large (cap at 512px on longest side for vocab cards)
        max_dim = max(w, h)
        if max_dim > 512:
            scale = 512 / max_dim
            img = img.resize((int(w * scale), int(h * scale)), Image.LANCZOS)
        img.save(output_path, "WEBP", quality=WEBP_QUALITY)
        return True
    except Exception:
        return False


def process_word(lemma, word_data, stock_word_id):
    """Download and convert the best image for a word. Returns accepted entry or None."""
    matches = word_data["matches"]
    # Sort by caption quality
    scored = [(score_caption(m["caption"], lemma), m) for m in matches]
    scored.sort(key=lambda x: x[0], reverse=True)

    for sc, m in scored:
        if sc < -3:
            continue  # skip non-photos
        url = m["image_url"]
        caption = m["caption"]

        # Download
        img_bytes = download_image(url)
        if img_bytes is None:
            continue

        # Convert to WebP
        url_hash = hashlib.md5(url.encode()).hexdigest()[:12]
        webp_name = f"cc3m_{word_data['word_id']}_{url_hash}.webp"
        webp_path = os.path.join(WEBP_DIR, webp_name)
        if not convert_to_webp(img_bytes, webp_path):
            continue

        return {
            "word_id": word_data["word_id"],
            "lemma": lemma,
            "caption": caption,
            "image_path": webp_path,
            "file_name": webp_name,
            "source_url": url,
        }

    return None


def main():
    # Load current stock-only words
    stock_words = load_stock_words()
    print(f"Current stock-only words: {len(stock_words)}", flush=True)

    # Load CC3M matches
    all_matches = json.load(open(MATCHES_PATH))
    print(f"CC3M matched lemmas (total): {len(all_matches)}", flush=True)

    # Filter to current stock-only words only
    to_process = {}
    for lemma, data in all_matches.items():
        if lemma in stock_words:
            data["word_id"] = stock_words[lemma]  # use current word_id
            to_process[lemma] = data

    print(f"CC3M matches for current stock-only words: {len(to_process)}", flush=True)

    os.makedirs(WEBP_DIR, exist_ok=True)
    os.makedirs(os.path.dirname(ACCEPTED_PATH), exist_ok=True)

    accepted = []
    failed = 0
    processed = 0

    # Process in parallel with ThreadPoolExecutor
    with ThreadPoolExecutor(max_workers=MAX_WORKERS) as pool:
        futures = {
            pool.submit(process_word, lemma, data, stock_words[lemma]): lemma
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
            except Exception as e:
                failed += 1

            if processed % 100 == 0:
                print(
                    f"  {processed}/{len(to_process)} processed, "
                    f"{len(accepted)} accepted, {failed} failed",
                    flush=True,
                )

    # Sort by word_id for deterministic output
    accepted.sort(key=lambda x: x["word_id"])

    with open(ACCEPTED_PATH, "w") as f:
        json.dump(accepted, f, indent=1)

    print(f"\n{'=' * 60}")
    print(f"CC3M PREPARE RESULTS")
    print(f"{'=' * 60}")
    print(f"Stock-only words with CC3M matches: {len(to_process)}")
    print(f"Successfully prepared:              {len(accepted)}")
    print(f"Failed (dead URLs / bad images):    {failed}")
    print(f"Accepted pairs written to:          {ACCEPTED_PATH}")
    print(f"{'=' * 60}")


if __name__ == "__main__":
    main()
