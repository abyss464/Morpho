# Morpho operator scripts

Reusable operator scripts, persisted out of the ephemeral /tmp scratchpad.
See `docs/OPERATIONS.md` for the full runbook. All assume the engine is up at
127.0.0.1:8787; CLIP scripts need `~/Code/vendor/ComfyUI/.venv/bin/python`.

- `bulk_approve.py` — approve every ready definition/example/image selection
- `clip_rematch.py` — CLIP-re-match images to slot-1 sentences (needs the venv)
- `mine_subs.py` — mine OpenSubtitles for per-lemma example sentences
