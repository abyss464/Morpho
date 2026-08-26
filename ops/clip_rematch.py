#!/usr/bin/env python3
"""CLIP image re-match against the upgraded slot-1 sentences.

For every active word, embed its available image candidates and its slot-1
sentence (+ lemma + trimmed definition as the text query), pick the candidate
with the highest image-text cosine — subject to the per-question distinctness
the app needs (a pick that visually duplicates an option already chosen in a
question this word participates in is skipped). Only re-select when the new
best beats the current selection's text-match by a margin, so we don't churn
already-good images. All mutations via the admin API.
"""

import json, sqlite3, urllib.request, urllib.error
import torch, open_clip
from PIL import Image

DB = "/home/abysser/Code/learning/Morpho/data/working.db"
MEDIA = "/home/abysser/Code/learning/Morpho/data/media"
API = "http://127.0.0.1:8787/api"
MARGIN = 0.02          # new pick must beat current text-match by this
DUP_SIM = 0.92         # visual-duplicate threshold within a question

def api(path, body):
    req = urllib.request.Request(API + path, data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "X-Morpho-User": "operator-clip"},
        method="POST")
    try:
        urllib.request.urlopen(req, timeout=30); return True
    except urllib.error.HTTPError as e:
        print("API", e.code, path, body.get("word_id")); return False

model, _, preprocess = open_clip.create_model_and_transforms(
    "ViT-B-32", pretrained="laion2b_s34b_b79k")
tok = open_clip.get_tokenizer("ViT-B-32")
model = model.cuda().eval()

conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)

words = conn.execute("""
    SELECT w.word_id, w.lemma FROM words w
    WHERE w.role='target' OR (w.role='auxiliary' AND w.aux_status='active')""").fetchall()
words = [(w, l) for w, l in words
         if conn.execute("SELECT 1 FROM words WHERE word_id=? AND zh_gloss IS NULL", (w,)).fetchone()]

def sentence(wid):
    r = conn.execute("""SELECT ec.text FROM example_selections es
        JOIN example_candidates ec ON ec.ex_cand_id=es.ex_cand_id
        WHERE es.word_id=? AND es.slot=1""", (wid,)).fetchone()
    return r[0] if r else None

def definition(wid):
    r = conn.execute("""SELECT dc.text FROM definition_selections ds
        JOIN definition_candidates dc ON dc.def_cand_id=ds.def_cand_id
        WHERE ds.word_id=? AND ds.is_primary=1 AND ds.enabled=1""", (wid,)).fetchone()
    return r[0] if r else ""

def img_embed(h):
    im = preprocess(Image.open(f"{MEDIA}/{h[:2]}/{h}.webp").convert("RGB")).unsqueeze(0).cuda()
    with torch.no_grad():
        f = model.encode_image(im); f = f / f.norm(dim=-1, keepdim=True)
    return f[0]

def txt_embed(text):
    with torch.no_grad():
        f = model.encode_text(tok([text]).cuda()); f = f / f.norm(dim=-1, keepdim=True)
    return f[0]

# preload current selection embeddings for duplicate checks
sel_hash = {w: h for w, h in conn.execute("""
    SELECT i.word_id, c.file_hash FROM image_selections i
    JOIN image_candidates c ON c.img_cand_id=i.img_cand_id""")}
emb_cache = {}
def emb_of(h):
    if h not in emb_cache:
        try: emb_cache[h] = img_embed(h)
        except Exception: emb_cache[h] = None
    return emb_cache[h]

# question membership: word -> set of other option words
qmates = {}
for w, d in conn.execute("SELECT word_id, distractor_word_id FROM distractors"):
    qmates.setdefault(w, set()).add(d)
    qmates.setdefault(d, set()).add(w)

changed = kept = noimg = 0
for i, (wid, lemma) in enumerate(words):
    cands = conn.execute("""SELECT img_cand_id, file_hash FROM image_candidates
        WHERE word_id=? AND status='available'""", (wid,)).fetchall()
    if not cands:
        noimg += 1; continue
    s = sentence(wid) or lemma
    query = f"{s} {lemma}: {definition(wid)[:80]}"
    q = txt_embed(query)
    cur = sel_hash.get(wid)
    mates = {sel_hash[m] for m in qmates.get(wid, ()) if m in sel_hash}
    scored = []
    for cid, h in cands:
        e = emb_of(h)
        if e is None: continue
        # reject if visually dup of a question-mate's current image
        if any((e @ emb_of(mh)) >= DUP_SIM for mh in mates if emb_of(mh) is not None):
            continue
        scored.append((float(q @ e), cid, h))
    if not scored:
        kept += 1; continue
    scored.sort(reverse=True)
    best_score, best_cid, best_h = scored[0]
    cur_score = next((sc for sc, _, h in scored if h == cur), None)
    if cur_score is not None and best_score <= cur_score + MARGIN:
        kept += 1; continue
    if api("/selections/image", {"word_id": wid, "cand_id": best_cid}) and \
       api("/selections/image/approve", {"word_id": wid}):
        sel_hash[wid] = best_h
        changed += 1
    if (i + 1) % 500 == 0:
        print(f"{i+1}/{len(words)}  changed {changed} kept {kept} noimg {noimg}", flush=True)

print(f"DONE changed {changed} kept {kept} noimg {noimg}", flush=True)
