# Morpho operator scripts

Engine: `http://127.0.0.1:30012`. CLIP scripts: `~/Code/vendor/ComfyUI/.venv/bin/python`.

- `bulk_approve.py` — approve all ready selections
- `unapprove_auto.py` — release auto-selected pins so scorer rebump can re-select
- `fix_primary_pos.py` — move primary sense onto corpus POS (`--dry-run` first)
- `mine_subs.py` — mine OpenSubtitles for per-lemma example sentences
- `coco_ingest.py` — ingest COCO Captions image-sentence pairs as native candidates
- `coco_revert_reimport.py` — revert bad COCO candidates and reimport with corrected offsets
- `verify_genimg.py` — verify codex-generated images against incumbents via CLIP, select winners
- `nsfw_screen.py` — NSFW and content-quality screening (CLIP probe, text-body, solid-color)
- `translate_sentences.py` — sentence translation
