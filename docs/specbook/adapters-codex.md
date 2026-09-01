# adapters/codex

8 specs.


============================================================
adapters/codex/src/morpho_codex/
============================================================


--- __init__.py ---

# Codex Adapter Package

Package entry point. Exports two things externally:

- **OPS** — The operation registry (dictionary) supported by this adapter. The engine uses it to look up callable operations.
- **OP_GENERATE** — Operation name constant `codex.generate`, so callers can reference it without hardcoding the string.


--- __main__.py ---

# Entry Point for Running the Package Directly

Running `python -m morpho_codex` executes the entry point, which is equivalent to the `codex-adapter` console script. It calls `cli.main()` to start the adapter process.


--- cli.py ---

# CLI entry point

The command-line entry point for the adapter, corresponding to the console script `codex-adapter`.

## main() -> None (directly exits the process)

Takes the adapter name `"codex"` and the operations registry, and passes them to the common command-line framework `morpho_adapter_common.run`. The framework takes over the event loop, and the exit code it returns is passed directly to `sys.exit`, ending the process.


--- image.py ---

# Image Post-Processing

Convert any-format images produced by the generator to the WebP specified in the release spec (768x576, q80). This aligns with the SDXL adapter target, except that letterbox padding is used here instead of stretching—since the generator's aspect ratio is uncontrollable, black bars are preferable to distortion. All metadata (EXIF, ICC) is discarded to ensure that identical content yields identical hashes in content-addressed storage.

## to_webp(source, out_path, width, height)

Decode the source image, scale it proportionally to fit within width x height (preserving the aspect ratio), pad the remaining area with black letterboxing, and write the result as WebP. The output is staged atomically and moved to the target path only after the write completes.

Raises a permanent error when the source file cannot be recognized as an image.

## Environment Variables

- `MORPHO_CODEX_WEBP_QUALITY` — overrides the default WebP quality (1-100); invalid values silently fall back to the default value of 80.


--- ops.py ---

# Operation Handler: codex.generate

This is the adapter's sole operation. It receives a word's context information, calls an external generator to create an image, performs quality inspection, and outputs standard WebP.

It's a four-step pipeline, three of which are gates: assembling the prompt → calling the generator → rejecting blank canvases → transcoding to the target path.

## generate(params) → dictionary

Generates an image for a word's vocabulary learning card.

**Required parameters:**
- `lemma` — the target word
- `slot1_sentence` — example sentence; the image depicts the scene described by this sentence (no more than 2000 characters)
- `out_path` — output file path, specified by the engine

**Optional parameters:**
- `prompt_ver` — prompt version, must match the current implementation version, otherwise rejected (default `codex/1`)
- `pos` — part of speech
- `primary_definition` — definition
- `width` / `height` — output dimensions (default 768x576, range 64-2048)

**Returns:**
- `model` — the model identifier used by the generator
- `prompt` — the actual prompt text sent

**Failure cases (permanent errors):**
- Prompt version mismatch
- Generator did not output a file (usually a content policy rejection)
- Output is a blank canvas (grayscale standard deviation below threshold)

## OPS

Operation registry in dictionary form, keyed by `"codex.generate"` with the generate function as its value. The adapter framework uses it for operation dispatch.


--- prompt.py ---

# Prompt wordbuild

takeword's context information is assembled into the instruction text sent to the generator. The wording follows the original version of `ops/genimg_cron.sh`—the 129 images produced by that instruction set all beat the old images they replaced on CLIP scores; rewriting would mean discarding the only evidence of effectiveness.

## CURRENT_VERSION

The current prompt template version constant, with value `"codex/1"`. The operation layer rejects any call whose requested version does not match this, ensuring that the candidate image's `source_ref` doesn't lie.

## build(lemma, slot1_sentence, out_path, width, height, pos=None, primary_definition=None) → string

Fills in the given parameters according to the current version template to generate a complete prompt text. The example is the main body—the image depicts the scene described by the example, while word and definition only serve to resolve ambiguity. No example means no prompt; this parameter is required.


--- quality.py ---

# Blank Image Detection

Ported from `ops/verify_genimg.py`'s quality gate. When the generator is rejected due to content policy or quota exhaustion, it sometimes doesn't report an error but instead writes out a pure white canvas. Such an image is worthless yet makes its way into the asset database, occupying a card slot until someone spots it with the naked eye.

Detection method: convert the image to grayscale and calculate the pixel standard deviation. Real photos are far above the threshold, while solid-color fills are near zero. The engine's own CLIP lower bound is responsible for intercepting "real images drawn incorrectly"; here we only intercept cases where "it isn't an image at all."

## is_blank(path, threshold=None) → boolean

Determines whether the image is a solid-color fill rather than a normal photo. A grayscale pixel standard deviation below the threshold is judged as blank. Files that cannot be decoded are also treated as blank — in either case, they're not worth putting in front of the learner.

## blank_threshold() → float

Returns the currently active blank detection threshold. Default is 0.5 (near zero on the 0-255 scale, only intercepting solid colors).

## Environment variable

- `MORPHO_CODEX_BLANK_STDDEV` — overrides the default blank detection threshold. Invalid values silently fall back to the default value.


--- runner.py ---

# Generator Driver

Calls the external codex CLI to perform image generation. Reproduces the call method from `ops/genimg_cron.sh`: it passes the prompt via stdin to `codex exec --dangerously-bypass-approvals-and-sandbox -C <dir> -`.

All call parameters can be overridden via environment variables, since the CLI is an external project and its command-line interface may change at any time. What cannot be overridden is error classification: binary not found → permanent error; non-zero exit → permanent error (the same prompt gets the same rejection); timeout → temporary error (the queue may simply be busy).

## available() → boolean

Checks whether the generator binary is reachable. Returns False when unreachable, without throwing an exception.

## generate(prompt, workdir) → string

Runs a single generation. Passes the prompt to the generator via stdin, running in `workdir`. Returns the model identifier string (recorded in the candidate image's metadata).

The generator writes files itself; this function is only responsible for driving and reporting the result. A normal exit with nothing written is a normal content-policy rejection, and is up to the caller to determine.

## Environment Variables

- `MORPHO_CODEX_BIN` — generator binary path or name (default `codex`)
- `MORPHO_CODEX_ARGS` — additional parameters in shell-quoted format, overriding the default `exec --dangerously-bypass-approvals-and-sandbox`
- `MORPHO_CODEX_TIMEOUT_S` — timeout in seconds (default 840, lower than the engine's 900-second job timeout, ensuring the adapter reports timeout first rather than being killed by the engine)
- `MORPHO_CODEX_MODEL` — model identifier recorded in the candidate image metadata (default `codex`)
