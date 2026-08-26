#!/usr/bin/env python3
"""CLIP image re-match against the upgraded slot-1 sentences.

For every active word, embed its available image candidates and its slot-1
sentence (+ lemma + trimmed definition as the text query), pick the candidate
with the highest image-text cosine — subject to the per-question distinctness
the app needs (a pick that visually duplicates an option already chosen in a
question this word participates in is skipped). Only re-select when the new
best beats the current selection's text-match by a margin, so we don't churn
already-good images. All mutations via the admin API.

Every word's outcome (score, chosen candidate, whether the run changed the
selection) is persisted to REPORT_PATH so a downstream pass can rank aptness
without recomputing embeddings. A genuinely human-reviewed pin — an
image_selection whose latest event actor is the admin-UI default identity
('admin:local'), as opposed to an operator/ops-script actor — is never
overridden, though it is still scored for the report.
"""

import json, os, sqlite3, urllib.request, urllib.error
import torch, open_clip
from PIL import Image

DB = "/home/abysser/Code/learning/Morpho/data/working.db"
MEDIA = "/home/abysser/Code/learning/Morpho/data/media"
API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012/api")
REPORT_PATH = os.environ.get(
    "CLIP_REPORT",
    "/tmp/claude-1000/-home-abysser-Code-learning-Morpho/"
    "d3d89616-3276-4389-b525-1b3f57774b29/scratchpad/clip_scores.json",
)
MARGIN = 0.02          # new pick must beat current text-match by this
DUP_SIM = 0.92         # visual-duplicate threshold within a question
# Actors that are automated pipeline/ops scripts, not a real reviewer using
# the admin gallery. The admin-ui client (admin-ui/src/api/client.ts) sends
# X-Morpho-User: local by default for genuine manual actions, recorded as
# actor 'admin:local'. Anything else touching image_selections in this
# dataset is an ops script or the reconciler.
HUMAN_ACTOR = "admin:local"

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
sel_cid = {w: c for w, c in conn.execute(
    "SELECT word_id, img_cand_id FROM image_selections")}

# words whose current image selection was last touched by a genuine human
# reviewer (admin gallery), per the events audit log — never overridden.
human_pinned = {wid for (wid,) in conn.execute("""
    SELECT entity_id FROM events e1
    WHERE entity_type='image_selection' AND actor=?
      AND event_id = (SELECT MAX(event_id) FROM events e2
                       WHERE e2.entity_type='image_selection'
                         AND e2.entity_id=e1.entity_id)""", (HUMAN_ACTOR,))}
human_pinned = {int(wid) for wid in human_pinned}

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

changed = kept = noimg = human_skipped = 0
report = []
for i, (wid, lemma) in enumerate(words):
    cands = conn.execute("""SELECT img_cand_id, file_hash FROM image_candidates
        WHERE word_id=? AND status='available'""", (wid,)).fetchall()
    if not cands:
        noimg += 1
        report.append({"word_id": wid, "lemma": lemma, "best_clip_score": None,
                        "selected_img_cand_id": None, "n_candidates": 0,
                        "changed": False})
        continue
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
        kept += 1
        report.append({"word_id": wid, "lemma": lemma, "best_clip_score": None,
                        "selected_img_cand_id": sel_cid.get(wid),
                        "n_candidates": len(cands), "changed": False})
        continue
    scored.sort(reverse=True)
    best_score, best_cid, best_h = scored[0]
    cur_score = next((sc for sc, _, h in scored if h == cur), None)

    if wid in human_pinned:
        # Never move a genuinely human-reviewed pick; still report its score.
        human_skipped += 1
        final_score = cur_score if cur_score is not None else best_score
        final_cid = sel_cid.get(wid)
        did_change = False
    elif cur_score is not None and best_score <= cur_score + MARGIN:
        kept += 1
        final_score, final_cid, did_change = cur_score, sel_cid.get(wid), False
    elif api("/selections/image", {"word_id": wid, "cand_id": best_cid}) and \
         api("/selections/image/approve", {"word_id": wid}):
        sel_hash[wid] = best_h
        sel_cid[wid] = best_cid
        changed += 1
        final_score, final_cid, did_change = best_score, best_cid, True
    else:
        # API call failed: nothing moved, report what's still live.
        kept += 1
        final_score, final_cid, did_change = cur_score, sel_cid.get(wid), False

    report.append({"word_id": wid, "lemma": lemma, "best_clip_score": final_score,
                    "selected_img_cand_id": final_cid, "n_candidates": len(cands),
                    "changed": did_change})

    if (i + 1) % 500 == 0:
        print(f"{i+1}/{len(words)}  changed {changed} kept {kept} noimg {noimg} "
              f"human_pinned {human_skipped}", flush=True)

os.makedirs(os.path.dirname(REPORT_PATH), exist_ok=True)
with open(REPORT_PATH, "w") as f:
    json.dump(report, f, indent=1)

print(f"DONE changed {changed} kept {kept} noimg {noimg} human_pinned {human_skipped}", flush=True)
print(f"report: {REPORT_PATH} ({len(report)} words)", flush=True)
