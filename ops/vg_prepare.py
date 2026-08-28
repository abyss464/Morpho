#!/usr/bin/env python3
"""Prepare Visual Genome region descriptions for Morpho ingest.

Takes:
  - VG region_descriptions.json (from Visual Genome dataset)
  - gap_words.tsv (word_id<TAB>lemma, from the working DB)
  - Downloaded VG images directory

Outputs:
  - ops/logs/vg-accepted.json (input for vg_ingest.py)
  - Coverage analysis report to stdout

For each gap word, finds VG descriptions containing the lemma as a whole
word, ranks by description quality (length, specificity, concreteness),
selects the top candidate, crops images to region bounding boxes, and
converts to WebP.
"""

import argparse
import json
import os
import re
import sys
from collections import defaultdict
from io import BytesIO
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    sys.exit("Pillow is required: pip install Pillow")


def load_gap_words(tsv_path):
    """Load gap words from TSV (word_id<TAB>lemma)."""
    words = {}
    with open(tsv_path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            parts = line.split("\t")
            if len(parts) < 2:
                continue
            word_id = int(parts[0])
            lemma = parts[1].lower()
            words[lemma] = word_id
    return words


_WORD_RE = re.compile(r"[a-zA-Z]+")


def build_region_index(vg_path, gap_lemmas):
    """Parse VG region descriptions and index by matching gap lemmas.

    Uses set-based token matching for speed: tokenize each phrase, check
    membership in the gap lemma set.  O(total_tokens) instead of
    O(regions * lemmas).

    Returns dict: lemma -> list of dicts with image_id, region_id, phrase, bbox.
    """
    print(f"Loading VG region descriptions from {vg_path}...", flush=True)

    lemma_set = {l.lower() for l in gap_lemmas}
    matches = defaultdict(list)
    total_regions = 0

    with open(vg_path) as f:
        data = json.load(f)

    print(f"Loaded {len(data)} images, scanning regions...", flush=True)

    for idx, img_entry in enumerate(data):
        image_id = img_entry["id"]
        for region in img_entry.get("regions", []):
            total_regions += 1
            phrase = region.get("phrase", "")
            if not phrase or len(phrase) < 5:
                continue

            # Tokenize and check set membership
            tokens = {t.lower() for t in _WORD_RE.findall(phrase)}
            hits = tokens & lemma_set
            if not hits:
                continue

            entry = {
                "image_id": image_id,
                "region_id": region.get("region_id", 0),
                "phrase": phrase,
                "x": region.get("x", 0),
                "y": region.get("y", 0),
                "width": region.get("width", 0),
                "height": region.get("height", 0),
            }
            for lemma in hits:
                matches[lemma].append(entry)

        if (idx + 1) % 20000 == 0:
            print(f"  {idx+1}/{len(data)} images...", flush=True)

    print(f"Scanned {total_regions} regions across {len(data)} images.", flush=True)
    return matches


def score_description(region, lemma):
    """Score a VG region description for suitability as vocabulary illustration.

    Higher is better. Considers:
    - Length (prefer 5-15 words — concrete, descriptive)
    - Bounding box area (larger regions show more context)
    - Avoidance of very short/generic phrases
    """
    phrase = region["phrase"]
    word_count = len(phrase.split())
    bbox_area = region["width"] * region["height"]

    score = 0.0

    # Phrase length: sweet spot is 5-12 words
    if 5 <= word_count <= 12:
        score += 3.0
    elif 3 <= word_count <= 15:
        score += 1.5
    elif word_count < 3:
        score -= 2.0

    # Bounding box area: larger regions produce better crops
    if bbox_area > 40000:
        score += 2.0
    elif bbox_area > 10000:
        score += 1.0
    elif bbox_area < 2000:
        score -= 1.0

    # Penalize obviously generic/fragmentary phrases
    if phrase.lower().strip() == lemma:
        score -= 3.0

    # Bonus for phrases starting with articles/demonstratives (more sentence-like)
    if re.match(r"^(a|an|the|this|that|some)\b", phrase, re.IGNORECASE):
        score += 0.5

    return score


def rank_and_select(matches, gap_words, max_per_word=3):
    """Rank matches per word and select the best candidates.

    Returns list of selected entries with word_id attached.
    """
    selected = []
    for lemma, word_id in gap_words.items():
        regions = matches.get(lemma, [])
        if not regions:
            continue

        # Score and sort
        scored = [(score_description(r, lemma), r) for r in regions]
        scored.sort(key=lambda x: x[0], reverse=True)

        # Deduplicate by image_id (one crop per image)
        seen_images = set()
        for _score, r in scored:
            if r["image_id"] in seen_images:
                continue
            seen_images.add(r["image_id"])
            selected.append({
                "word_id": word_id,
                "lemma": lemma,
                "caption": r["phrase"],
                "vg_image_id": r["image_id"],
                "vg_region_id": r["region_id"],
                "region_bbox": {
                    "x": r["x"], "y": r["y"],
                    "width": r["width"], "height": r["height"],
                },
            })
            if len(seen_images) >= max_per_word:
                break

    return selected


def crop_and_convert(selected, images_dir, output_dir, webp_quality=80):
    """Crop images to region bounding boxes and convert to WebP.

    Updates each entry in `selected` with an `image_path` field pointing
    to the output WebP file, or removes entries whose source image is
    missing or whose crop is invalid.
    """
    os.makedirs(output_dir, exist_ok=True)
    valid = []
    missing_images = 0
    crop_errors = 0

    for entry in selected:
        image_id = entry["vg_image_id"]
        bbox = entry["region_bbox"]

        # VG images are in VG_100K/ or VG_100K_2/ — try both
        src = None
        for subdir in ["", "VG_100K", "VG_100K_2"]:
            candidate = os.path.join(images_dir, subdir, f"{image_id}.jpg")
            if os.path.isfile(candidate):
                src = candidate
                break

        if src is None:
            missing_images += 1
            continue

        try:
            img = Image.open(src)
            img_w, img_h = img.size

            # Clamp bounding box to image dimensions
            x = max(0, bbox["x"])
            y = max(0, bbox["y"])
            w = min(bbox["width"], img_w - x)
            h = min(bbox["height"], img_h - y)

            if w < 50 or h < 50:
                crop_errors += 1
                continue

            cropped = img.crop((x, y, x + w, y + h))

            # Resize if too small — target min dimension 256px
            min_dim = min(cropped.size)
            if min_dim < 256 and min_dim > 0:
                scale = 256 / min_dim
                new_size = (int(cropped.size[0] * scale), int(cropped.size[1] * scale))
                cropped = cropped.resize(new_size, Image.LANCZOS)

            out_name = f"vg_{image_id}_r{entry['vg_region_id']}.webp"
            out_path = os.path.join(output_dir, out_name)
            cropped.save(out_path, "WEBP", quality=webp_quality)

            entry["image_path"] = out_path
            entry["file_name"] = out_name
            valid.append(entry)
        except Exception as e:
            crop_errors += 1
            print(f"  crop error image_id={image_id}: {e}", file=sys.stderr)
            continue

    print(f"Cropped: {len(valid)} valid, {missing_images} missing images, "
          f"{crop_errors} crop errors", flush=True)
    return valid


def print_coverage_report(gap_words, matches, selected):
    """Print a coverage analysis report."""
    total_gap = len(gap_words)
    matched_words = {lemma for lemma in gap_words if matches.get(lemma)}
    matched_count = len(matched_words)

    print("\n" + "=" * 60)
    print("VISUAL GENOME COVERAGE ANALYSIS")
    print("=" * 60)
    print(f"\nGap words (no image scoring >= 0.22 CLIP):  {total_gap}")
    print(f"Gap words with VG description matches:      {matched_count}")
    print(f"Coverage rate:                              {matched_count/total_gap*100:.1f}%")
    print(f"Gap words NOT matched:                      {total_gap - matched_count}")

    # Distribution of matches per word
    match_counts = [len(matches.get(lemma, [])) for lemma in gap_words if matches.get(lemma)]
    if match_counts:
        match_counts.sort()
        print(f"\nMatches per word (among matched words):")
        print(f"  Min:    {match_counts[0]}")
        print(f"  Median: {match_counts[len(match_counts)//2]}")
        print(f"  Mean:   {sum(match_counts)/len(match_counts):.1f}")
        print(f"  Max:    {match_counts[-1]}")
        print(f"  Total VG region matches: {sum(match_counts)}")

        # Histogram
        buckets = [(1, 1), (2, 5), (6, 20), (21, 100), (101, 500), (501, float('inf'))]
        print(f"\n  Distribution:")
        for lo, hi in buckets:
            count = sum(1 for c in match_counts if lo <= c <= hi)
            label = f"{lo}" if lo == hi else f"{lo}-{int(hi)}" if hi != float('inf') else f"{lo}+"
            print(f"    {label:>6s} matches: {count:>4d} words")

    # Sample matches
    print(f"\nSample matches (15 representative examples):")
    print(f"{'Word':<15s} {'VG Image ID':>12s} Description")
    print("-" * 75)

    sample_lemmas = sorted(matched_words)
    # Pick spread across the list
    step = max(1, len(sample_lemmas) // 15)
    samples = sample_lemmas[::step][:15]
    for lemma in samples:
        regions = matches[lemma]
        best = max(regions, key=lambda r: score_description(r, lemma))
        print(f"{lemma:<15s} {best['image_id']:>12d} {best['phrase'][:45]}")

    # Quality assessment
    print(f"\nDescription quality assessment:")
    all_phrases = []
    for lemma in matched_words:
        for r in matches[lemma][:5]:
            all_phrases.append(r["phrase"])

    if all_phrases:
        avg_len = sum(len(p.split()) for p in all_phrases) / len(all_phrases)
        concrete = sum(1 for p in all_phrases if len(p.split()) >= 4) / len(all_phrases)
        print(f"  Average phrase length: {avg_len:.1f} words (sample of {len(all_phrases)})")
        print(f"  Phrases >= 4 words (concrete): {concrete*100:.1f}%")
        print(f"  Phrases are human-written region descriptions of photographs,")
        print(f"  grounded in visible content — suitable for vocabulary illustration.")

    if selected:
        selected_words = {e["lemma"] for e in selected}
        print(f"\nSelected for ingest: {len(selected)} pairs across {len(selected_words)} words")

    print("=" * 60)


def main():
    parser = argparse.ArgumentParser(description="Prepare VG data for Morpho ingest")
    parser.add_argument("--vg-json", default="/tmp/vg/region_descriptions.json",
                        help="Path to VG region_descriptions.json")
    parser.add_argument("--gap-words", default="/tmp/vg/gap_words.tsv",
                        help="Path to gap_words.tsv")
    parser.add_argument("--images-dir", default="/tmp/vg/images",
                        help="Directory containing downloaded VG images")
    parser.add_argument("--output-dir", default="/tmp/vg/crops",
                        help="Directory for cropped WebP outputs")
    parser.add_argument("--max-per-word", type=int, default=1,
                        help="Max candidates per word (default 1)")
    parser.add_argument("--analysis-only", action="store_true",
                        help="Run coverage analysis only, skip crop/convert")
    parser.add_argument("--webp-quality", type=int, default=80,
                        help="WebP quality (default 80)")
    args = parser.parse_args()

    ops_dir = os.path.dirname(os.path.abspath(__file__))

    # Load gap words
    gap_words = load_gap_words(args.gap_words)
    print(f"Loaded {len(gap_words)} gap words.", flush=True)

    # Build region index
    matches = build_region_index(args.vg_json, set(gap_words.keys()))

    # Rank and select
    selected = rank_and_select(matches, gap_words, max_per_word=args.max_per_word)

    # Print coverage report
    print_coverage_report(gap_words, matches, selected)

    if args.analysis_only:
        # Write match data for later use
        match_summary_path = "/tmp/vg/match_summary.json"
        summary = {}
        for lemma in gap_words:
            regions = matches.get(lemma, [])
            summary[lemma] = {
                "word_id": gap_words[lemma],
                "match_count": len(regions),
                "top_phrases": [r["phrase"] for r in sorted(
                    regions, key=lambda r: score_description(r, lemma), reverse=True
                )[:5]],
            }
        with open(match_summary_path, "w") as f:
            json.dump(summary, f, indent=1)
        print(f"\nMatch summary written to {match_summary_path}")
        return

    # Crop and convert images
    if os.path.isdir(args.images_dir):
        selected = crop_and_convert(selected, args.images_dir, args.output_dir,
                                     webp_quality=args.webp_quality)
    else:
        print(f"\nImages directory {args.images_dir} not found — skipping crop step.")
        print("Download matched images first, then re-run without --analysis-only.")
        return

    # Write accepted JSON
    accepted_path = os.path.join(ops_dir, "logs", "vg-accepted.json")
    os.makedirs(os.path.dirname(accepted_path), exist_ok=True)
    with open(accepted_path, "w") as f:
        json.dump(selected, f, indent=1)
    print(f"\nAccepted pairs written to {accepted_path}")


if __name__ == "__main__":
    main()
