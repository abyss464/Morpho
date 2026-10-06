"""Score many (sentence, pictures) jobs with the engine's own CLIP model.

Runs inside the morphod container, under the CLIP adapter's virtualenv, so the
scores are on the same ruler as the ones the engine stores:

  docker compose exec -T morphod /app/adapters/clip/.venv/bin/python - JOBS OUT < ops/clip_batch.py

JOBS is a JSON list of {"key", "text", "images": [path, ...]}; OUT receives
{"key": {"scores": {path: cosine}, "flags": {path: probe}}}. Every picture is
also compared against an inappropriate-content probe battery (the same one
`nsfw_screen.py` uses); `flags` names the probe that beats every neutral probe.
The model loads once for the whole batch.
"""

import json
import sys

sys.path.insert(0, "/app/adapters/clip/src")

from morpho_clip.scorer import OpenClipScorer  # noqa: E402

INAPPROPRIATE = [
    "a nude person",
    "a naked human body",
    "sexually explicit photo",
    "a sexually suggestive photo",
    "a dead body, corpse",
    "a violent physical assault",
    "a disturbing gory injury",
]
NEUTRAL = [
    "a person wearing clothes",
    "a photo of an everyday object or scene",
    "a person doing an everyday activity",
    "a medical illustration or diagram",
    "a landscape or nature photo",
    "a portrait of a person's face",
]


def main():
    jobs_path, out_path = sys.argv[1], sys.argv[2]
    jobs = json.load(open(jobs_path))
    scorer = OpenClipScorer()
    torch, model, preprocess, tokenizer, device = scorer._load()
    from PIL import Image

    with torch.no_grad():
        probe = model.encode_text(tokenizer(INAPPROPRIATE + NEUTRAL).to(device))
        probe = probe / probe.norm(dim=-1, keepdim=True)
        out = {}
        for n, job in enumerate(jobs, 1):
            query = model.encode_text(tokenizer([job["text"]]).to(device))
            query = query / query.norm(dim=-1, keepdim=True)
            scores, flags = {}, {}
            paths, tensors = [], []
            for path in job["images"]:
                try:
                    with Image.open(path) as handle:
                        tensors.append(preprocess(handle.convert("RGB")))
                    paths.append(path)
                except Exception:
                    continue
            if paths:
                feats = model.encode_image(torch.stack(tensors).to(device))
                feats = feats / feats.norm(dim=-1, keepdim=True)
                for path, feat in zip(paths, feats):
                    scores[path] = float(query[0] @ feat)
                    sims = (probe @ feat).tolist()
                    bad = max(range(len(INAPPROPRIATE)), key=lambda i: sims[i])
                    if sims[bad] > max(sims[len(INAPPROPRIATE):]):
                        flags[path] = INAPPROPRIATE[bad]
            out[str(job["key"])] = {"scores": scores, "flags": flags}
            if n % 100 == 0:
                print(f"scored {n}/{len(jobs)}", file=sys.stderr, flush=True)
    json.dump(out, open(out_path, "w"))


main()
