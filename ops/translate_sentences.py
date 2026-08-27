#!/usr/bin/env python3
"""Machine-translate en->zh the slot-1 example sentence of every active word.

Reads the read-only working DB, translates each sentence with `trans`
(translate-shell, already installed on this machine) and writes
`admin-ui/public/sentence-zh.json` as `{"<word_id>": "<中文>"}` for the
gallery's image-review mode.

Why translate-shell and not an offline neural model (e.g. Argos Translate):
Argos Translate's dependency chain on this machine pulls in torch, triton
and spacy (~1GB+ of wheels) purely for its optional sentence-splitter, and
unpacking it saturated at a few hundred KB/s — multiple hours just to
install, before a single sentence was translated. `trans` was already on
the machine, is a thin scriptable wrapper around a translation engine, and
needs no LLM/agent tokens spent on translation (a 20-sentence spot check
came back 0 failures, ~1.4s/sentence). Traded offline-ness for something
that actually finishes; every translation is still produced by the
deterministic tool, never guessed by the agent.

Usage:
    python3 ops/translate_sentences.py [--db PATH] [--out PATH] [--workers N]
                                        [--engine google|bing] [--limit N]

Reusable: rerun whenever slot-1 sentences change. Always re-translates every
current slot-1 sentence from scratch; at ~4-5k short sentences and a handful
of parallel workers this finishes in several minutes, so incremental
caching isn't worth the complexity.
"""

from __future__ import annotations

import argparse
import json
import shutil
import sqlite3
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_DB = REPO_ROOT / "data" / "working.db"
DEFAULT_OUT = REPO_ROOT / "admin-ui" / "public" / "sentence-zh.json"

FROM_LANG = "en"
TO_LANG = "zh"
MAX_ATTEMPTS = 4
RETRY_ENGINES = ["google", "google", "bing", "yandex"]  # escalate engine on repeat failure


def fetch_sentences(db_path: Path) -> list[tuple[int, str]]:
    """Every active word's selected slot-1 sentence."""
    uri = f"file:{db_path}?mode=ro"
    conn = sqlite3.connect(uri, uri=True)
    try:
        rows = conn.execute(
            """
            SELECT w.word_id, ec.text
            FROM active_words w
            JOIN example_selections es ON es.word_id = w.word_id AND es.slot = 1
            JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
            ORDER BY w.word_id
            """
        ).fetchall()
    finally:
        conn.close()
    return [(int(word_id), text) for word_id, text in rows]


def translate_one(text: str) -> str | None:
    """Best-effort en->zh translation of one sentence via `trans`, retrying
    across engines. Returns None if every attempt failed."""
    import subprocess

    for attempt in range(MAX_ATTEMPTS):
        engine = RETRY_ENGINES[min(attempt, len(RETRY_ENGINES) - 1)]
        try:
            proc = subprocess.run(
                ["trans", "-b", "-e", engine, f"{FROM_LANG}:{TO_LANG}", text],
                capture_output=True,
                text=True,
                timeout=20,
            )
        except subprocess.TimeoutExpired:
            time.sleep(1.5 * (attempt + 1))
            continue
        zh = proc.stdout.strip()
        if proc.returncode == 0 and zh:
            return zh
        time.sleep(1.5 * (attempt + 1))
    return None


def translate_all(pairs: list[tuple[int, str]], workers: int) -> tuple[dict[str, str], list[int]]:
    result: dict[str, str] = {}
    failed: list[int] = []
    total = len(pairs)
    started = time.time()
    done = 0

    with ThreadPoolExecutor(max_workers=workers) as pool:
        futures = {pool.submit(translate_one, text): word_id for word_id, text in pairs}
        for future in as_completed(futures):
            word_id = futures[future]
            zh = future.result()
            done += 1
            if zh is None:
                failed.append(word_id)
            else:
                result[str(word_id)] = zh
            if done % 200 == 0 or done == total:
                elapsed = time.time() - started
                rate = done / elapsed if elapsed > 0 else 0
                print(
                    f"  {done}/{total} translated ({elapsed:.0f}s elapsed, {rate:.1f}/s, "
                    f"{len(failed)} failed so far)",
                    file=sys.stderr,
                )
    return result, failed


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", type=Path, default=DEFAULT_DB, help="Path to working.db")
    parser.add_argument(
        "--out", type=Path, default=DEFAULT_OUT, help="Output path for sentence-zh.json"
    )
    parser.add_argument("--workers", type=int, default=6, help="Concurrent translate-shell calls")
    parser.add_argument("--limit", type=int, default=None, help="Translate only the first N rows (testing)")
    parser.add_argument(
        "--retry-missing",
        action="store_true",
        help="Load --out if it exists, translate only word_ids missing from it, and merge "
        "the results back in. For recovering from a mid-run failure batch (e.g. a "
        "rate-limit window) without repaying the whole run.",
    )
    args = parser.parse_args()

    if shutil.which("trans") is None:
        print("error: `trans` (translate-shell) is not on PATH.", file=sys.stderr)
        raise SystemExit(1)

    pairs = fetch_sentences(args.db)
    if args.limit:
        pairs = pairs[: args.limit]

    existing: dict[str, str] = {}
    if args.retry_missing and args.out.exists():
        existing = json.loads(args.out.read_text(encoding="utf-8"))
        before = len(pairs)
        pairs = [(word_id, text) for word_id, text in pairs if str(word_id) not in existing]
        print(
            f"--retry-missing: {len(existing)} already translated, "
            f"{len(pairs)}/{before} still missing.",
            file=sys.stderr,
        )

    print(f"{len(pairs)} slot-1 sentences to translate ({FROM_LANG}->{TO_LANG}).", file=sys.stderr)
    if not pairs:
        print("Nothing to translate; leaving any existing output file untouched.", file=sys.stderr)
        return

    result, failed = translate_all(pairs, args.workers)
    result = {**existing, **result}

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(
        json.dumps(result, ensure_ascii=False, sort_keys=True) + "\n", encoding="utf-8"
    )
    total_target = len(existing) + len(pairs) if args.retry_missing else len(pairs)
    print(f"Wrote {len(result)}/{total_target} translations to {args.out}", file=sys.stderr)
    if failed:
        print(
            f"{len(failed)} word_ids failed after {MAX_ATTEMPTS} attempts each: {failed}",
            file=sys.stderr,
        )


if __name__ == "__main__":
    main()
