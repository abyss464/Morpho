#!/usr/bin/env bash
# Recurring codex image-generation batch for Morpho.
#
# Every firing: find words from the wave list that still lack a generated
# image, hand the next batch to codex (non-interactive), then ingest the
# results (upload + CLIP verify + select) via ops/verify_genimg.py.
# Self-terminates the systemd timer when the list is exhausted.
#
# State lives next to the images: done.json marks word_ids already ingested,
# so a word whose generated image LOST to its incumbent is not retried
# forever.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRATCH="${MORPHO_GENIMG_DIR:-$ROOT/ops/logs/genimg}"
GENDIR="$SCRATCH/genimg2"
WORDS="$SCRATCH/image_gen_wave2.json"
LOGDIR="$ROOT/ops/logs"
LOG="$LOGDIR/genimg_cron.log"
BATCH=40
CLIP_PY="$HOME/Code/vendor/ComfyUI/.venv/bin/python"
# systemd user services don't inherit fnm's shell PATH — pin the node install.
NODE_BIN="$HOME/.local/share/fnm/node-versions/v24.14.1/installation/bin"
export PATH="$NODE_BIN:$PATH"
CODEX="$NODE_BIN/node $HOME/.local/share/fnm/node-versions/v24.14.1/installation/lib/node_modules/@openai/codex/bin/codex.js"

mkdir -p "$LOGDIR" "$GENDIR"
exec >>"$LOG" 2>&1
echo "=== $(date -Is) firing ==="

if [ ! -f "$WORDS" ]; then
    echo "word list $WORDS missing (scratchpad wiped?) — disabling timer"
    systemctl --user disable --now morpho-genimg.timer
    exit 0
fi

# Words still needing generation: no PNG on disk and not marked done.
REMAINING_JSON="$GENDIR/remaining.json"
python3 - "$WORDS" "$GENDIR" "$REMAINING_JSON" "$BATCH" <<'EOF'
import json, os, sys
words_path, gendir, out_path, batch = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
words = json.load(open(words_path))
done = set()
done_path = os.path.join(gendir, "done.json")
if os.path.exists(done_path):
    done = set(json.load(open(done_path)))
remaining = [w for w in words
             if not os.path.exists(os.path.join(gendir, f"{w['word_id']}.png"))
             and w["word_id"] not in done]
json.dump(remaining[:batch], open(out_path, "w"), ensure_ascii=False, indent=1)
print(f"remaining={len(remaining)} batch={len(remaining[:batch])}")
EOF

COUNT=$(python3 -c "import json;print(len(json.load(open('$REMAINING_JSON'))))")
if [ "$COUNT" -eq 0 ]; then
    echo "list exhausted — disabling timer"
    systemctl --user disable --now morpho-genimg.timer
    exit 0
fi

echo "generating $COUNT images via codex"
$CODEX exec --dangerously-bypass-approvals-and-sandbox -C "$SCRATCH" - <<PROMPT
Generate vocabulary-learning illustration images using your image generation capability.

Read the JSON file at $REMAINING_JSON — a list of word entries with fields word_id, lemma, pos, primary_definition, slot1_sentence.

For EACH entry, generate one image saved as $GENDIR/{word_id}.png.

Requirements: generate a photorealistic scene image, NOT text, NOT words, NOT letters, NOT a white background. The image must depict a VISUAL SCENE described by the example sentence (slot1_sentence), showing the MEANING of the word (lemma) as used there, guided by primary_definition — a learner must be able to pick it out of 4 images as matching the sentence. Photographic or realistic illustration, clear single subject, landscape ~768x576 (4:3). NEVER generate an image that contains any written text, typography, or letters — no captions, no labels, no signage, no numbers, nothing legible, anywhere in the frame. For abstract words use a concrete instantly-readable metaphor, still rendered as a real photographic scene, never as a text/diagram/icon.

Process in order. If a generation fails, retry once; on a second quota-type failure STOP immediately; on a content-policy failure skip that word and continue.
PROMPT

GENERATED=$(command ls "$GENDIR" | grep -c '\.png$' || true)
echo "png count now: $GENERATED"

echo "ingesting via verify_genimg.py"
MORPHO_GENIMG_DIR="$GENDIR" \
MORPHO_GENIMG_WORDS="$WORDS" \
MORPHO_GENIMG_RESULTS="$GENDIR/results-$(date +%s).json" \
"$CLIP_PY" "$ROOT/ops/verify_genimg.py"
INGEST_RC=$?
echo "ingest rc=$INGEST_RC"

# Mark every word with a PNG as done (uploaded; dedupe makes re-upload a no-op
# but skipping them keeps batches clean).
python3 - "$WORDS" "$GENDIR" <<'EOF'
import json, os, sys
words_path, gendir = sys.argv[1], sys.argv[2]
words = json.load(open(words_path))
done_path = os.path.join(gendir, "done.json")
done = set()
if os.path.exists(done_path):
    done = set(json.load(open(done_path)))
for w in words:
    if os.path.exists(os.path.join(gendir, f"{w['word_id']}.png")):
        done.add(w["word_id"])
json.dump(sorted(done), open(done_path, "w"))
print(f"done marks: {len(done)}")
EOF

echo "=== $(date -Is) complete ==="
