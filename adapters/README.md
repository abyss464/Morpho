# adapters/

Stateless Python CLIs that morphod spawns one process per job. Each speaks the
envelope in `docs/contracts/adapter-protocol.md`: one JSON request on stdin, one
JSON response on stdout, logs on stderr, exit 0 whenever the protocol was
honored.

| Package | Op | External dependency |
|---|---|---|
| `common/` | — (shared envelope, error taxonomy, param validation, atomic output) | none |
| `tts/` | `tts.synthesize` | edge-tts (network) + **ffmpeg with libopus** |
| `morfessor/` | `morfessor.segment` | Morfessor 2.x (pure Python, offline) |
| `sdxl/` | `sdxl.generate` | a local ComfyUI HTTP server |

Each adapter is its own uv project with its own lockfile and virtualenv, and
depends on `common/` through a path source. Nothing is shared at runtime, so
running the sdxl adapter never drags edge-tts onto disk.

## How morphod invokes them

```
uv run --project adapters/tts        tts-adapter
uv run --project adapters/morfessor  morfessor-adapter
uv run --project adapters/sdxl       sdxl-adapter
```

Write the request to stdin, close it, read one line of JSON from stdout. `python
-m morpho_tts` / `morpho_morfessor` / `morpho_sdxl` are equivalent entry points
if the environment is already activated.

Timeouts are morphod's to enforce (tts 60 s, morfessor 120 s/batch, sdxl 600 s);
each adapter also self-limits a little below its budget so a slow dependency
comes back as a classified error rather than an opaque kill.

```console
$ echo '{"op":"morfessor.segment","params":{"words":["benevolent","unhappiness"]}}' \
    | uv run --project adapters/morfessor morfessor-adapter
{"ok":true,"result":{"model_ver":"morfessor/2.0.6+nomodel","segments":{...}}}
```

### Exit codes

| Code | Meaning |
|---|---|
| `0` | The protocol was honored. stdout holds one JSON line — `ok: true` **or** an error response. |
| `2` | Protocol failure: stdin was unreadable, not JSON, not an object, or carried no string `op`. stdout is empty; stderr explains. |

An **unknown op** is a `permanent` error response with exit 0, not a crash: the
envelope was fine, and retrying a request this binary cannot serve is pointless.
An **unexpected exception** inside a handler becomes `transient` — the adapter
cannot tell a flaky dependency from its own bug, and morphod's bounded retries
end in a dead letter either way. The traceback goes to stderr for `last_error`.

### Error taxonomy

| Kind | morphod's response | Adapters emit it for |
|---|---|---|
| `permanent` | record and never retry | bad params, unknown op, missing ffmpeg, unreachable ComfyUI, a graph the backend cannot run |
| `transient` | exponential backoff | network faults, 5xx, timeouts, unexpected exceptions |
| `rate_limited` | park the lane until `retry_after_ms` | HTTP 429 and throttling language from edge-tts or ComfyUI |

`retry_after_ms` is always present and is `0` unless the upstream named a
cooldown; rate limits without a `Retry-After` default to 60 s.

### Output files

morphod passes `out_path` (a temp path it owns), hashes the result and renames it
into the content-addressed store. Adapters never touch `data/`. Writes land on a
sibling `.part` file and are renamed into place only once complete, so a crashed
adapter never leaves a truncated file at `out_path`.

## Running the tests

No test in this tree opens a socket. edge-tts, ffmpeg and ComfyUI are all faked.

From the repo root, name the suite explicitly:

```console
$ uv run --project adapters/common     pytest adapters/common/tests
$ uv run --project adapters/tts        pytest adapters/tts/tests
$ uv run --project adapters/morfessor  pytest adapters/morfessor/tests
$ uv run --project adapters/sdxl       pytest adapters/sdxl/tests
```

`uv run --directory adapters/<name> pytest` is equivalent and shorter. A bare
`uv run --project adapters/<name> pytest` does **not** work: `--project` selects
the environment but leaves the working directory alone, so pytest roots itself at
the repo root, never reads the adapter's `[tool.pytest.ini_options]`, and tries to
collect all four suites into one venv. Either form above pins the rootdir to the
adapter and picks up its config.

Lint the same way — `uv run --project adapters/<name> ruff check adapters/<name>`
and `ruff format --check adapters/<name>`.

## ffmpeg

The tts adapter requires **ffmpeg built with libopus** on `PATH`. edge-tts hands
back mp3; the release format is mono Ogg Opus (README part 5), so every
synthesis is transcoded before `out_path` is written.

A missing ffmpeg, or an ffmpeg without libopus, is a `permanent` error with an
actionable message — retrying will not install it. Availability is checked
*before* the network call, so an unconfigured box fails in milliseconds instead
of paying for synthesis it cannot use.

```console
$ ffmpeg -hide_banner -encoders | grep libopus   # expect: A....D libopus
```

Encoder settings are pinned (`-ac 1 -ar 48000 -c:a libopus -vbr on
-application audio -frame_duration 20 -compression_level 10`) and all metadata
and encoder tags are stripped via `-fflags +bitexact -flags +bitexact`, so the
same mp3 always transcodes to the same bytes. Verified locally: two runs over
one source produce identical sha256.

## Environment variables

### All adapters

| Variable | Default | Effect |
|---|---|---|
| `MORPHO_ADAPTER_LOG_LEVEL` | `INFO` | stderr log level (`DEBUG`, `INFO`, `WARNING`, `ERROR`, `CRITICAL`). |

### tts

| Variable | Default | Effect |
|---|---|---|
| `MORPHO_FFMPEG` | `ffmpeg` on `PATH` | Explicit ffmpeg binary. |
| `MORPHO_TTS_TIMEOUT_S` | `45` | Budget for the edge-tts stream (morphod kills at 60 s). |
| `MORPHO_TTS_FFMPEG_TIMEOUT_S` | `30` | Budget for one transcode. |

Request params: `text`, `voice` (both required), `rate` (`+0%`), `pitch`
(`+0Hz`), `format` (`ogg_opus`, the only supported value), `bitrate_kbps` (`32`),
`out_path`. `volume` (`+0%`) is accepted as an optional extension beyond the
contract; it maps to edge-tts's volume control.

`duration_ms` is read from the Ogg granule position minus the Opus pre-skip —
the exact decoded length, no ffprobe subprocess. It runs a few milliseconds
shorter than the container duration ffprobe reports, which includes the pre-skip
padding.

### morfessor

| Variable | Default | Effect |
|---|---|---|
| `MORPHO_MORFESSOR_MODEL` | — | Explicit model file, overriding directory discovery. |
| `MORPHO_MORFESSOR_MIN_VOCAB` | `100` | Distinct words required before ad-hoc training is attempted. `0` disables the floor. |
| `MORPHO_MORFESSOR_MORPH_LENGTH` | `5.0` | Target characters per morph for the auto-tuned corpus weight. |
| `MORPHO_MORFESSOR_MAX_EPOCHS` | `8` | Ad-hoc training epoch cap. |

**The wave-1 model is a dev stopgap.** With no model file present the adapter
trains a throwaway Morfessor Baseline on the batch it was handed. Real corpus
training is a later wave; drop the resulting model into `adapters/morfessor/
model/` (see that directory's README) and the adapter uses it instead, with no
code change.

The stopgap is deterministic — fixed seed, sorted and deduplicated training set,
so batch order cannot change the answer — and honest about its provenance:

| `model_ver` | Meaning |
|---|---|
| `morfessor/2.0.6+model-2026-08` | Pretrained model, tagged from the `VERSION` sidecar (or a digest of the file). |
| `morfessor/2.0.6+adhoc-<digest>` | Ad-hoc model. The digest covers the training words *and* the hyperparameters, so two batches never share a version and morphod's input hashes invalidate correctly. |
| `morfessor/2.0.6+nomodel` | No usable model; words returned unsegmented. Carries no digest because the output does not depend on the batch. |

Two guards keep the stopgap from writing nonsense into `etymology`, and both
apply to ad-hoc models only — a pretrained model's output passes through as-is:

- below `MORPHO_MORFESSOR_MIN_VOCAB` distinct words there is no morphological
  signal to learn, so words come back unsegmented;
- if the trained model produces morphs averaging under 2.5 characters it is
  declared degenerate and its output is discarded the same way.

Both guards are why a two-word request answers `{"benevolent": ["benevolent"]}`
rather than shattering the word into letters. Batches of a few hundred words and
up segment normally (measured ~3 s for 1 900 words; the batch cap is 5 000).

### sdxl

| Variable | Default | Effect |
|---|---|---|
| `COMFYUI_URL` | `http://127.0.0.1:8188` | ComfyUI base URL. |
| `MORPHO_SDXL_CHECKPOINT` | from the template | Checkpoint filename; also becomes the response's `model`. |
| `MORPHO_SDXL_WORKFLOW` | bundled template | Path to an alternative workflow template. |
| `MORPHO_SDXL_TIMEOUT_S` | `540` | Total render budget (morphod kills at 600 s). |
| `MORPHO_SDXL_POLL_INTERVAL_S` | `1.5` | `/history` poll interval. |
| `MORPHO_SDXL_CONNECT_TIMEOUT_S` | `3` | Reachability probe timeout. |
| `MORPHO_SDXL_WEBP_QUALITY` | `80` | WebP quality, per the release budget. |
| `MORPHO_SDXL_NATIVE_BUCKETS` | `1` | Set `0` to render at the requested size instead of an SDXL-native bucket. |

The adapter probes `GET /system_stats` before submitting anything. A dead socket
is reported as `permanent` with the contract's exact message, `sdxl backend not
configured`, so the word surfaces in dead letters instead of retry-looping. Any
HTTP answer — including a 404 from a ComfyUI too old to have `/system_stats` —
counts as configured.

SDXL was trained at roughly one megapixel, so asking it directly for 768x576
renders off-distribution. The adapter generates in the nearest native bucket
(768x576 is 4:3, so 1152x896) and downscales with LANCZOS to exactly the
requested size. WebP output is quality 80, method 6, with all metadata stripped
so identical renders hash identically.

#### Workflow templates

`src/morpho_sdxl/workflows/sdxl_txt2img_v1.json` is a literal ComfyUI API-format
graph under a `graph` key, plus a `bindings` map naming which node input each
request parameter drives:

```json
"bindings": {
  "checkpoint":      ["4", "ckpt_name"],
  "positive_prompt": ["6", "text"],
  "negative_prompt": ["7", "text"],
  "seed":            ["3", "seed"],
  "width":           ["5", "width"],
  "height":          ["5", "height"]
}
```

The graph stays literal so it can be pasted straight into ComfyUI to debug a bad
render. Sampler settings are pinned (`dpmpp_2m` / `karras` / 30 steps / cfg 7.0);
changing any of them changes the pixels, so it gets a **new file** —
`sdxl_txt2img_v2.json` — never an edit in place.

The default checkpoint is `sd_xl_base_1.0.safetensors`. If your ComfyUI does not
have that file, `POST /prompt` answers 400 with `node_errors` and the adapter
reports it as `permanent` — a configuration fault, not a blip.
