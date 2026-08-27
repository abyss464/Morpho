# CLIP Service Contract (morphod ⇄ adapters/clip)

The CLIP sidecar answers one question: **how well does this picture answer this
sentence?** morphod folds the answer into image selection, where it is the
dominant term (`morpho_reconcile::score::CLIP_WEIGHT`).

## Why it is a service and not an adapter

The other three adapters are subprocesses: morphod spawns one per job, writes a
JSON request to stdin and reads a JSON response from stdout. This one is not,
for two reasons that are both about where the hardware is.

**The GPU is not in the container.** morphod ships in `python:3.12-slim` with no
accelerator and no ROCm runtime. Spawning `uv run --project adapters/clip` inside
it would spawn a process that cannot load the model at all. `adapters/sdxl`
already solved this: the GPU work runs *outside* the engine and is reached over
HTTP. The only difference here is who wrote the server — ComfyUI is somebody
else's, so `adapters/sdxl` is a thin client for it; CLIP has no such server, so
`adapters/clip` is the server.

**The model load dwarfs the work.** Scoring a word's pool is a handful of
milliseconds; building `ViT-B-32` and moving it onto the device is seconds. A
process per job would spend all its time loading a model it then uses once.

So morphod reaches it with `reqwest`, exactly as it reaches the Free Dictionary
or Wikimedia, and the same `Permanent | Transient | RateLimited` taxonomy
applies: 4xx is permanent, 5xx and transport failures are transient, 429 parks
the lane.

## What crosses the wire

Content hashes, never bytes. The media library is content addressed and the
sidecar is given its own root, so a request names `file_hash` values and the
sidecar resolves `{root}/{hash[:2]}/{hash}.webp` itself. Scoring a whole pool
costs one small JSON round trip instead of a multi-megabyte upload, and the
container's `/app/data/media` and the host's `data/media` are free to be spelled
differently — a hash means the same file either way, a path would not.

## Endpoints

### `GET /health`

```json
{"ok": true, "algo_ver": "clip/1", "model": "ViT-B-32/laion2b_s34b_b79k",
 "media_root": "/home/…/data/media"}
```

### `POST /score`

```json
{"text": "She had to abandon the car in the flood.",
 "images": ["a1b2…", "c3d4…"]}
→ {"model": "ViT-B-32/laion2b_s34b_b79k", "algo_ver": "clip/1",
   "scores": [{"file_hash": "a1b2…", "similarity": 0.2731}],
   "missing": ["c3d4…"]}
```

* `text` — the word's selected slot-1 sentence, or its lemma when it has none.
  The same formula `ops/clip_rematch.py` used, and for the same reason: the
  mode-1 card shows that sentence beside four pictures, so it is the question the
  picture has to answer. It is scored **verbatim** — the sidecar must not trim or
  case-fold it, because morphod files the answer under a hash of the text it
  sent.
* `images` — at most 64 content hashes. Order is preserved in `scores`, which is
  the only thing tying a number to a picture.
* `similarity` — the cosine of the two L2-normalized embeddings, in `[-1, 1]`.
  In practice a ViT-B-32 image-text cosine lands between about 0.08 and 0.32.
* `missing` — hashes the library could not produce. **Reported, never fatal**:
  one file lost to a restore must not cost a word the scores of its other
  candidates. morphod logs the count and stores what came back.

A malformed request is `400` (permanent — it will not become readable on a
retry). A backend failure is `500` (transient).

## Identity, and why it is checked

`clip_scores.model_ver` is `"{algo_ver}:{model}"`, and it is part of the row's
primary key. That is what makes the artifact content addressed rather than
stale-able: a different model writes *different rows*, and the old ones lose
their readers — the same mechanism `tts_assets` uses for a voice change.

Every reply carries its identity, and morphod refuses one that is not the
identity it stores under (`images.clip_model` / `MORPHO_CLIP_MODEL`). A sidecar
serving a different checkpoint would file two models' cosines in one column with
nothing able to tell them apart, which the version check prevents. The refusal is
permanent, not a warning.

## Running it

```bash
PYTHONPATH=<repo>/adapters/clip/src \
~/Code/vendor/ComfyUI/.venv/bin/python -m morpho_clip \
  --media-root <repo>/data/media --host 0.0.0.0 --port 30013
```

Under a venv that already holds a working accelerator build of torch plus
`open_clip` — `adapters/clip` declares no dependencies of its own precisely so it
can be run from a venv this repository does not own. `--host 0.0.0.0` only when
morphod is containerized; a native morphod wants the `127.0.0.1` default.

Then `MORPHO_CLIP_URL=http://host.docker.internal:30013` (the compose file maps
that name to the host gateway) or `http://127.0.0.1:30013` natively.

## Degradation

Every part of this is optional, and the absence of each is an ordinary state
rather than a failure:

| Absent | Effect |
|---|---|
| `MORPHO_CLIP_URL` unset | No `score_image_clip` job is ever derived. Image selection ranks on the quality prior alone — bit for bit the behaviour that predates semantic scoring. |
| Sidecar down | Jobs fail transiently, back off, and eventually dead-letter. Words already scored keep their scores; unscored words rank on quality. Nothing is lost. |
| One word unscored | That word's pool is ranked on quality alone. The semantic term is per word and all-or-nothing, so a half-scored pool is never ranked on two different rulers. |
| A picture missing from disk | Reported in `missing`; its siblings are still scored, and the word falls back to quality ranking until the pool is complete. |
