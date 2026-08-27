# adapters/codex — generative image source of last resort

Serves `codex.generate`: prompt an external image generator for a picture of the
scene one word's slot-1 sentence describes, gate out the ones that are not
pictures, and write the WebP morphod hashes into the library.

This is the last link in the image chain. Every stock library, both keyless
second passes and local SDXL generation come first; a word only reaches here
when CLIP says its best picture still does not answer its own sentence. See
`docs/contracts/adapter-protocol.md` for the op and README part 4 for the chain.

The prompt is `ops/genimg_cron.sh`'s, carried over rather than rewritten — those
instructions produced the 129 images in release 1.5, every one of which beat the
incumbent it replaced. The blank-canvas gate is `ops/verify_genimg.py`'s, at the
same threshold. What is *not* here is the CLIP comparison that script also did:
the engine has CLIP now and does that itself, which is the whole point of the
move.

## Configuration

| Variable | Default | Effect |
|---|---|---|
| `MORPHO_CODEX_BIN` | `codex` | The generator binary. morphod checks the same variable, so an absent one disables the source rather than dead-lettering every word. |
| `MORPHO_CODEX_ARGS` | `exec --dangerously-bypass-approvals-and-sandbox` | Arguments before the `-` that puts the prompt on stdin. Shell-quoted. |
| `MORPHO_CODEX_MODEL` | `codex` | Label recorded on the candidate's `source_ref`. |
| `MORPHO_CODEX_TIMEOUT_S` | `840` | Below morphod's own 900 s job timeout, so a slow queue reports as a timeout rather than being killed. |
| `MORPHO_CODEX_BLANK_STDDEV` | `0.5` | Grayscale stddev under which a render is a flat colour field. |
| `MORPHO_CODEX_WEBP_QUALITY` | `80` | README part 5's budget. |

## Testing

```bash
uv run --directory adapters/codex pytest
```

Nothing here spends a quota: `MORPHO_CODEX_BIN` points at a stub that behaves
like the real CLI in each of the cases that matter — it draws, it refuses
silently, it writes a blank canvas, it fails.
