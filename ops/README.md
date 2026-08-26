# Morpho operator scripts

Reusable operator scripts, persisted out of the ephemeral /tmp scratchpad.
See `docs/OPERATIONS.md` for the full runbook. The older scripts assume a native
engine at 127.0.0.1:8787; compose publishes the containerised one on 30012, so
point them there when the engine runs under Docker. CLIP scripts need
`~/Code/vendor/ComfyUI/.venv/bin/python`.

- `bulk_approve.py` — approve every ready definition/example/image selection
- `unapprove_auto.py` — release the pins approval put on auto-selected slots, so
  a `scorer_ver` bump can actually re-select (re-approve afterwards)
- `clip_rematch.py` — CLIP-re-match images to slot-1 sentences (needs the venv)
- `mine_subs.py` — mine OpenSubtitles for per-lemma example sentences
