#!/usr/bin/env python3
"""Widen the image pools of authored words whose pictures miss their sentence.

The engine ranks a word's pictures by how well CLIP says they answer the word's
slot-1 sentence. When even the best picture in a pool scores under the inapt
bar, the pool lacks a matching photo. This script:

  1. scores every authored word's pool against its slot-1 sentence with the
     engine's own CLIP model (loaded once, inside the morphod container);
  2. for words under the bar, searches the keyless photo libraries (Wikimedia
     Commons, Openverse) with the sentence's content words;
  3. scores the results the same way, drops any the inappropriate-content probe
     flags, and uploads the best few that beat the pool.

Nothing is selected here: the engine re-scores the uploads and its selector
picks, exactly as for a fetched photo. Generated pictures are never searched.

  ops/expand_images.py [--threshold 0.22] [--limit N] [--dry-run] FILE...

FILE is an authored definitions file; its primary lines name the words. Words
already handled are recorded in ops/logs/expand_images.jsonl (with each
upload's origin and licence) and skipped on a re-run.
"""

import argparse
import concurrent.futures as cf
import hashlib
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DB = os.path.join(ROOT, "data/working.db")
API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012")
LOG = os.path.join(ROOT, "ops/logs/expand_images.jsonl")
STAGE = os.environ.get("EXPAND_STAGE", "/tmp/morpho-expand")
CONTAINER_STAGE = os.environ.get("EXPAND_CONTAINER_STAGE", "/tmp/expand")
UA = "MorphoStudy/1.0 (personal vocabulary app)"
CHUNK = 200
# Pictures are scored from Wikimedia's standard 330px thumbnail bucket (cached,
# so fast and not throttled) and the few that win are fetched again at 960px,
# the width the engine itself asks Commons for.
SCORE_WIDTH = 330
UPLOAD_WIDTH = 960
TIMEOUT = 15
PER_WORD_RESULTS = 16
# The engine's CLIP model; the pools' pictures are mostly scored against slot 1 already.
MODEL_VER = "clip/1:ViT-B-32/laion2b_s34b_b79k"
# Pixabay finds at least this many: enough to choose from without the slow libraries.
PIXABAY_ENOUGH = 8
UPLOADS_PER_WORD = 2
MARGIN = 0.02

STOP = set("""
a an the and or but if so of to in on at by for with from into onto over under up down out off about as than then
is are was were be been being am do does did done have has had having will would can could shall should may might must
not no nor this that these those there here it its it's he she they we you i me him her them us my your his our their
what which who whom whose when where why how all any some every each very too also just only even still yet again
one two three four five first last more most much many few other another such own same new old good bad big small
get got gets make made makes take took go goes went going come came put say said says tell told give gave
like well always never often sometimes usually now today yesterday tomorrow after before because while until
across against through around behind between without within along toward towards near away back
need needs want wants know knew think thought see saw look looks looked use used find found keep kept
""".split())


def api_upload(word_id, path, source):
    out = subprocess.run(
        ["curl", "-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST", f"{API}/api/candidates/image",
         "-H", "X-Morpho-User: expand-images", "-F", f"word_id={word_id}", "-F", f"source={source}",
         "-F", f"file=@{path}"],
        capture_output=True, text=True,
    )
    return out.stdout.strip()


class Throttle:
    """Paces every request to one host, shared by all threads.

    Requests leave at least `interval` apart. A 429 or 503 pauses the host for
    its Retry-After and doubles the interval; every success narrows it again
    toward the base, so the run settles at whatever rate the provider actually
    allows instead of hammering it or crawling after one refusal.
    """

    def __init__(self, interval):
        self.base = interval
        self.interval = interval
        self.next_at = 0.0
        self.lock = threading.Lock()
        self.limited = 0

    def wait(self):
        with self.lock:
            now = time.monotonic()
            at = max(now, self.next_at)
            self.next_at = at + self.interval
        time.sleep(max(0.0, at - now))

    def succeed(self):
        with self.lock:
            self.interval = max(self.base, self.interval * 0.95)

    def penalize(self, retry_after):
        with self.lock:
            self.limited += 1
            self.interval = min(self.interval * 2, 10.0)
            self.next_at = max(self.next_at, time.monotonic() + retry_after)


THROTTLES = {
    "pixabay.com": Throttle(0.65),
    # Pixabay's limit is on its API; the picture files it links to are fetched freely.
    "pixabay.com/get": Throttle(0.05),
    "commons.wikimedia.org": Throttle(0.5),
    "api.openverse.org": Throttle(1.0),
    "upload.wikimedia.org": Throttle(0.1),
}
DEFAULT_THROTTLE = Throttle(0.05)


def request(url, attempts=5):
    """GET through the host's throttle, retrying rate limits; other errors raise."""
    parts = urllib.parse.urlsplit(url)
    host = parts.hostname
    if host == "pixabay.com" and not parts.path.startswith("/api"):
        host = "pixabay.com/get"
    throttle = THROTTLES.get(host, DEFAULT_THROTTLE)
    for attempt in range(attempts):
        throttle.wait()
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        try:
            with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
                data = resp.read(24 * 1024 * 1024)
            throttle.succeed()
            return data
        except urllib.error.HTTPError as err:
            if err.code not in (429, 503) or attempt == attempts - 1:
                raise
            try:
                retry_after = float(err.headers.get("Retry-After") or 30)
            except ValueError:
                retry_after = 30.0
            throttle.penalize(min(retry_after, 120.0))
    raise RuntimeError("unreachable")


def http_json(url):
    return json.loads(request(url))


def fetch(url):
    return request(url, attempts=3)


def content_words(sentence, lemma):
    words = re.findall(r"[A-Za-z][A-Za-z'-]*", sentence)
    seen, out = set(), []
    for w in words:
        low = w.lower().strip("'")
        if low in STOP or len(low) < 3 or low in seen:
            continue
        seen.add(low)
        out.append(low)
    head = [w for w in out if w.startswith(lemma.lower()[: max(3, len(lemma) - 2)])]
    rest = [w for w in out if w not in head]
    return head[:1], rest


def queries_for(sentence, lemma):
    head, rest = content_words(sentence, lemma)
    qs = []
    if rest:
        qs.append(" ".join(rest[:3]))
        qs.append(" ".join(rest[:2]))
        qs.append(" ".join([lemma] + rest[:1]))
    qs.append(lemma)
    out = []
    for q in qs:
        if q and q not in out:
            out.append(q)
    return out


def search_commons(q):
    params = {
        "action": "query", "format": "json", "generator": "search", "gsrnamespace": "6",
        "gsrsearch": f"{q} filetype:bitmap", "gsrlimit": "10", "prop": "imageinfo",
        "iiprop": "url|mime|size|extmetadata", "iiurlwidth": str(SCORE_WIDTH),
        "iiextmetadatafilter": "LicenseShortName|Artist",
    }
    data = http_json("https://commons.wikimedia.org/w/api.php?" + urllib.parse.urlencode(params))
    out = []
    for page in (data.get("query") or {}).get("pages", {}).values():
        info = (page.get("imageinfo") or [{}])[0]
        if info.get("mime") != "image/jpeg" or (info.get("width") or 0) < 400 or (info.get("height") or 0) < 300:
            continue
        meta = info.get("extmetadata") or {}
        artist = re.sub(r"<[^>]+>", "", (meta.get("Artist") or {}).get("value", "")).strip()
        thumb = info.get("thumburl")
        full = thumb.replace(f"/{SCORE_WIDTH}px-", f"/{UPLOAD_WIDTH}px-") if thumb and info["width"] > UPLOAD_WIDTH else info.get("url")
        out.append({
            "source": "wikimedia", "thumb": thumb, "full": full, "page": info.get("descriptionurl"),
            "license": (meta.get("LicenseShortName") or {}).get("value", ""), "author": artist[:200], "query": q,
        })
    return out


class Openverse:
    def __init__(self):
        self.alive = True

    def search(self, q):
        if not self.alive:
            return []
        url = "https://api.openverse.org/v1/images/?" + urllib.parse.urlencode(
            {"q": q, "page_size": "10", "mature": "false", "extension": "jpg"})
        try:
            data = http_json(url)
        except urllib.error.HTTPError as err:
            if err.code in (401, 403):
                self.alive = False
                return []
            raise
        out = []
        for r in data.get("results", []):
            if (r.get("width") or 640) < 400:
                continue
            out.append({
                "source": "openverse", "thumb": r.get("thumbnail") or r.get("url"), "full": r.get("url"),
                "page": r.get("foreign_landing_url"),
                "license": f"{r.get('license', '')} {r.get('license_version', '')}".strip(),
                "author": (r.get("creator") or "")[:200], "query": q,
            })
        return out


def pixabay_key():
    """Pixabay's free API key, from the environment or the compose .env file;
    re-read every chunk so a key added mid-run is picked up."""
    key = os.environ.get("PIXABAY_API_KEY", "").strip()
    env = os.path.join(ROOT, ".env")
    if not key and os.path.exists(env):
        for line in open(env):
            name, _, value = line.partition("=")
            if name.strip() == "PIXABAY_API_KEY":
                key = value.strip().strip('"').strip("'")
    return key or None


def search_pixabay(q, key):
    url = "https://pixabay.com/api/?" + urllib.parse.urlencode(
        {"key": key, "q": q[:100], "image_type": "photo", "safesearch": "true", "per_page": "20"})
    out = []
    for r in http_json(url).get("hits", []):
        if (r.get("imageWidth") or 0) < 400:
            continue
        out.append({
            "source": "pixabay", "thumb": r.get("webformatURL"), "full": r.get("largeImageURL"),
            "page": r.get("pageURL"), "license": "Pixabay Content License", "author": (r.get("user") or "")[:200],
            "query": q,
        })
    return out


def gather(word, openverse, key):
    """Search results for one word, or None when a search failed outright (the
    word is then left for a later run rather than recorded as searched).

    Pixabay, when a key is configured, is asked first with every query because
    its limit is generous. The keyless libraries allow this machine only a few
    requests a minute, so they are asked only for words Pixabay leaves short."""
    results, seen = [], set()

    def take(found):
        for r in found:
            if r["thumb"] and r["thumb"] not in seen:
                seen.add(r["thumb"])
                results.append(r)

    try:
        if key:
            for q in word["queries"]:
                take(search_pixabay(q, key))
                if len(results) >= PER_WORD_RESULTS:
                    break
            if len(results) >= PIXABAY_ENOUGH:
                return results[:PER_WORD_RESULTS]
        for q in word["queries"]:
            take(search_commons(q))
            take(openverse.search(q))
            if len(results) >= PER_WORD_RESULTS:
                break
    except Exception:
        return None
    return results[:PER_WORD_RESULTS]


def download(result, stage):
    try:
        data = fetch(result["thumb"])
    except Exception:
        return None
    if len(data) < 8000:
        return None
    digest = hashlib.sha256(data).hexdigest()
    path = os.path.join(stage, f"{digest}.jpg")
    with open(path, "wb") as fh:
        fh.write(data)
    return dict(result, file=f"{digest}.jpg")


def full_size(found):
    """The winner at upload size; the scoring thumbnail when that fails."""
    path = os.path.join(STAGE, found["file"])
    if found.get("full") and found["full"] != found["thumb"]:
        try:
            data = fetch(found["full"])
            if len(data) > 8000:
                path = os.path.join(STAGE, "full-" + found["file"])
                with open(path, "wb") as fh:
                    fh.write(data)
        except Exception:
            pass
    return path


def clip_score(jobs, stage):
    """Score jobs inside the container; image paths are stage-relative or media-relative."""
    with open(os.path.join(stage, "jobs.json"), "w") as fh:
        json.dump(jobs, fh)
    subprocess.run(["docker", "compose", "exec", "-T", "morphod", "rm", "-rf", CONTAINER_STAGE], cwd=ROOT, check=True)
    subprocess.run(["docker", "compose", "cp", stage, f"morphod:{CONTAINER_STAGE}"], cwd=ROOT, check=True,
                   stdout=subprocess.DEVNULL)
    with open(os.path.join(ROOT, "ops/clip_batch.py")) as script:
        subprocess.run(
            ["docker", "compose", "exec", "-T", "morphod", "/app/adapters/clip/.venv/bin/python", "-",
             f"{CONTAINER_STAGE}/jobs.json", f"{CONTAINER_STAGE}/out.json"],
            cwd=ROOT, check=True, stdin=script,
        )
    subprocess.run(["docker", "compose", "cp", f"morphod:{CONTAINER_STAGE}/out.json", os.path.join(stage, "out.json")],
                   cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["docker", "compose", "exec", "-T", "morphod", "rm", "-rf", CONTAINER_STAGE], cwd=ROOT, check=True)
    with open(os.path.join(stage, "out.json")) as fh:
        return json.load(fh)


def authored_examples(paths):
    examples = {}
    for path in paths:
        for line in open(path):
            parts = [p.strip() for p in line.split(" | ")]
            if len(parts) >= 5 and parts[2] == "*":
                examples[int(parts[0])] = parts[4]
    return examples


def load_words(examples, done):
    conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    words, pending = [], 0
    for wid, example in examples.items():
        if wid in done:
            continue
        lemma = conn.execute("SELECT lemma FROM words WHERE word_id = ?", (wid,)).fetchone()[0]
        row = conn.execute(
            "SELECT ec.text, ec.text_hash FROM example_selections es"
            " JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id"
            " WHERE es.word_id = ? AND es.slot = 1", (wid,)).fetchone()
        if row is None or " ".join(row[0].split()) != " ".join(example.split()):
            pending += 1
            continue
        pool = [r[0] for r in conn.execute(
            "SELECT ic.file_hash FROM image_candidates ic JOIN media_files m ON m.file_hash = ic.file_hash"
            " WHERE ic.word_id = ? AND ic.status = 'available'", (wid,))]
        rel = {h: conn.execute("SELECT rel_path FROM media_files WHERE file_hash = ?", (h,)).fetchone()[0] for h in pool}
        stored = dict(conn.execute(
            "SELECT ic.file_hash, cs.similarity FROM image_candidates ic"
            " JOIN clip_scores cs ON cs.file_hash = ic.file_hash AND cs.text_hash = ? AND cs.model_ver = ?"
            " WHERE ic.word_id = ? AND ic.status = 'available'", (row[1], MODEL_VER, wid)))
        words.append({"word_id": wid, "lemma": lemma, "text": row[0], "pool": rel, "stored": stored,
                      "queries": queries_for(row[0], lemma)})
    if pending:
        print(f"{pending} words skipped: slot 1 does not hold the authored example yet", flush=True)
    return words


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="+")
    ap.add_argument("--threshold", type=float, default=0.22)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    done = set()
    if os.path.exists(LOG):
        for line in open(LOG):
            done.add(json.loads(line)["word_id"])
    words = load_words(authored_examples(args.files), done)
    if args.limit:
        words = words[: args.limit]
    print(f"{len(words)} words to check", flush=True)
    openverse = Openverse()
    totals = {"checked": 0, "under": 0, "uploaded": 0, "improved": 0, "failed": 0}

    for start in range(0, len(words), CHUNK):
        chunk = words[start: start + CHUNK]
        shutil.rmtree(STAGE, ignore_errors=True)
        os.makedirs(STAGE)
        # Pass 1: the pools as they stand, from the engine's stored scores; only
        # pictures it has not scored yet go through CLIP here.
        jobs = [{"key": w["word_id"], "text": w["text"],
                 "images": [f"/app/data/{p}" for h, p in w["pool"].items() if h not in w["stored"]]} for w in chunk]
        jobs = [j for j in jobs if j["images"]]
        scored = clip_score(jobs, STAGE) if jobs else {}
        under = []
        for w in chunk:
            s = list(w["stored"].values()) + list(scored.get(str(w["word_id"]), {}).get("scores", {}).values())
            w["best"] = max(s) if s else 0.0
            if w["best"] < args.threshold:
                under.append(w)
        # Pass 2: search, download, score the under-bar words' finds.
        with cf.ThreadPoolExecutor(8) as pool:
            key = pixabay_key()
            finds = list(pool.map(lambda w: gather(w, openverse, key), under))
        shutil.rmtree(STAGE, ignore_errors=True)
        os.makedirs(STAGE)
        with cf.ThreadPoolExecutor(16) as pool:
            for w, found in zip(under, finds):
                w["failed"] = found is None
                w["finds"] = [d for d in pool.map(lambda r: download(r, STAGE), found or []) if d]
        jobs = [{"key": w["word_id"], "text": w["text"],
                 "images": [f"{CONTAINER_STAGE}/{d['file']}" for d in w["finds"]]} for w in under if w["finds"]]
        scored = clip_score(jobs, STAGE) if jobs else {}
        under_ids = {w["word_id"] for w in under}
        with open(LOG, "a") as log:
            for w in chunk:
                if w.get("failed"):
                    totals["failed"] += 1
                    continue
                record = {"word_id": w["word_id"], "lemma": w["lemma"], "best_before": round(w["best"], 4), "uploads": []}
                if w["word_id"] in under_ids:
                    res = scored.get(str(w["word_id"]), {"scores": {}, "flags": {}})
                    ranked = sorted(
                        ((res["scores"][f"{CONTAINER_STAGE}/{d['file']}"], d) for d in w["finds"]
                         if f"{CONTAINER_STAGE}/{d['file']}" in res["scores"]
                         and f"{CONTAINER_STAGE}/{d['file']}" not in res["flags"]),
                        key=lambda x: -x[0])
                    for score, d in ranked[:UPLOADS_PER_WORD]:
                        if score < w["best"] + MARGIN:
                            break
                        status = "dry" if args.dry_run else api_upload(w["word_id"], full_size(d), d["source"])
                        record["uploads"].append({"score": round(score, 4), "status": status, "source": d["source"],
                                                  "page": d["page"], "license": d["license"], "author": d["author"],
                                                  "query": d["query"]})
                    if record["uploads"]:
                        totals["improved"] += 1
                        totals["uploaded"] += len(record["uploads"])
                if not args.dry_run:
                    log.write(json.dumps(record, ensure_ascii=False) + "\n")
                elif w["word_id"] in under_ids:
                    print(json.dumps(record, ensure_ascii=False))
        totals["checked"] += len(chunk)
        totals["under"] += len(under)
        pace = {h: (round(t.interval, 2), t.limited) for h, t in THROTTLES.items()}
        print(f"... {totals} pace={pace}", flush=True)
    shutil.rmtree(STAGE, ignore_errors=True)
    print(f"done {totals} openverse={'on' if openverse.alive else 'off'}")


if __name__ == "__main__":
    sys.exit(main())
