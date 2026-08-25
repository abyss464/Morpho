# Morfessor model directory

Drop a trained Morfessor model here and the adapter uses it instead of the
wave-1 ad-hoc stopgap. Nothing else in this directory is read.

- **File**: any `*.bin`, `*.model` or `*.gz` in this directory. If several are
  present the lowest-sorting name wins, so prefer exactly one. `MorfessorIO.
  read_any_model` accepts both binary (pickled) and text segmentation formats.
- **`VERSION`** (optional): a one-line corpus vintage such as `2026-08`. It
  becomes the tag in `model_ver` (`morfessor/2.0.6+model-2026-08`), matching the
  example in `docs/contracts/adapter-protocol.md`. Without it the adapter falls
  back to a digest of the model file's bytes.
- **`MORPHO_MORFESSOR_MODEL`** overrides discovery with an explicit file path.

Model files are binary artifacts, not source. Keep them out of git — build them
in the corpus-training job planned for a later wave and ship them alongside the
adapter.
