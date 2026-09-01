# adapters/clip

8 specs.


============================================================
adapters/clip/src/morpho_clip/
============================================================


--- __init__.py ---

# CLIP Adapter Package Entry Point

Image-text relevance scoring adapter. Provides two operation modes: subprocess adapter (primary use, spawned per task) and HTTP sidecar service (auxiliary, long-lived GPU service).

## Re-export List

Collectively export the following symbols from submodules:

- **MediaLibrary** / **default_root** / **is_content_hash** — content-addressed media database
- **OPS** — the subprocess adapter's operation registry
- **Scorer** / **OpenClipScorer** / **ALGO_VER** — scoring protocol, implementation, and algorithm version number
- **ScoreService** / **build_handler** / **serve** / **BadRequestError** — HTTP service layer


--- __main__.py ---

# Package Script Entry Point

When running `python -m morpho_clip`, execute the following lines: call the `cli` module's `main` function to start the CLIP sidecar HTTP service, using its return code as the process exit code.


--- adapter.py ---

# Subprocess adapter entry point

The entry point of the `clip-adapter` console script. Morphod tasks spawn this subprocess, following the adapter-protocol envelope protocol.

## main() → no return (exits directly)

Passes the adapter name `clip` and the operation registry OPS to morpho_adapter_common's generic runner. The runner reads envelopes from stdin, dispatches them to the corresponding operations, writes results back to stdout, and finally terminates the process with the exit code returned by the runner.


--- cli.py ---

# Sidecar CLI

`clip-sidecar` (or `python -m morpho_clip`)'s command-line interface. Starts a long-running HTTP scoring service, for the engine to call the CLIP model via network.

## Command-line parameters

- `--host` — listening address, default 127.0.0.1
- `--port` — listening port, default 30013
- `--media-root` — content-addressed media database path; if not specified, first read the environment variable MORPHO_CLIP_MEDIA_ROOT, then fall back to data/media
- `--arch` — open_clip model architecture
- `--pretrained` — pretrained weights identifier
- `--device` — torch device; if not specified, use CUDA if available, otherwise use CPU
- `--quiet` — lower the log level to only output WARNING and above

## main(argv) → int

Parse parameters, build MediaLibrary and ScoreService, then start the HTTP service. If the media database root directory does not exist, output a warning but still start normally. Return 0.


--- media.py ---

# Content-Addressed Media Database

Resolves content hashes to image files on disk. The media database stores files in a sharded structure of `{root}/{hash[:2]}/{hash}.{ext}`. The engine only passes hashes; this layer returns read-only bytes — no candidate-word or word-data databases are involved.

## is_content_hash(value) → bool

Determines whether a string is a valid content address (a 64-character hexadecimal BLAKE3 digest). Invalid values are rejected before path concatenation to prevent path traversal.

## default_root() → Path

Default path of the media database. Reads the `MORPHO_CLIP_MEDIA_ROOT` environment variable first; falls back to `data/media` when it is not set.

## MediaLibrary

Read-only view of the content-addressed media store.

### Constructor

Takes the media database root directory path.

### path_for(file_hash) → Path or None

Finds the file corresponding to a hash and returns its path. Tries extensions in order: webp, png, jpg, jpeg. If the hash format is invalid or the file does not exist, returns `None` without raising an exception — the caller will add it to the missing list. A missing image does not affect the scores of other candidate images under the same word.


--- ops.py ---

# clip.score Operation

The subprocess adapter's only operation, following the adapter-protocol envelope protocol. The engine sends a set of content hashes and a media database path, and this returns the cosine similarity between each image and the text.

Each call reloads the model — this is the already-known cost of subprocess mode, in exchange for operational reliability (previously, the resident-memory sidecar couldn't score thousands of images after each restart).

## score(params) → dict

Performs a CLIP score on a word's candidate images.

**Input parameters:**
- text — the text to score (slot-1 example or word metadata), sent to the model as-is
- images — list of content hashes of candidate images, nullable
- media_root — absolute path of the content-addressed media database

**Returns:**
- model — model identifier, format: architecture/pretrained weights
- algo_ver — algorithm version number
- scores — each image's hash and similarity
- missing — hashes not found in the media database

**Limits:**
- images accepts at most 64 items
- text is at most 2000 characters; exceeding the limit reports a permanent error

When the image list is empty or all images are missing, directly return the model identifier without loading the model.

## OPS

Operation registry, mapping the operation name `clip.score` to the score function. Passed to morpho_adapter_common's common runner.


--- scorer.py ---

# CLIP Scorer

Model layer. Defines the score protocol and the open_clip implementation. The embedding logic reproduces the computation in `ops/clip_rematch.py`: encoding, L2 normalization, dot product—the engine's thresholds are read directly from that script's values; changing the normalization method will silently shift all thresholds.

## ALGO_VER

Algorithm version constant, value `clip/1`. Must match the engine-side `CLIP_ALGO_VER`; on mismatch, the engine will reject at the first request.

## Scorer (Protocol)

All requirements imposed by the service layer on the model.

### model (property) -> str

Model identifier, formatted as architecture/pretrained weights, for the engine to verify identity.

### score(text, paths) -> float list

Computes the cosine similarity between `text` and each image, returning results in the order passed in.

## OpenClipScorer

Based on open_clip's actual implementation. `torch` and `open_clip` are imported lazily—when the model is not loaded, there is no dependency on an accelerator environment.

### Constructor

Receives the model architecture, pretrained weight identifier, and optional torch device. The model is loaded only at the first score request; the loading process is thread-safe.

### model (property) -> str

Same as defined in the protocol.

### score(text, paths) -> float list

Same as defined in the protocol. Encodes each image individually and computes the cosine with the text embedding, running under `torch.no_grad`.


--- service.py ---

# HTTP ScoreService

Two endpoints, zero frameworks. Use the standard library's `http.server` implementation, because this service must run in an external venv (the one holding the accelerated torch), and every extra dependency is one more package installed in someone else's environment.

## ScoreService

Request-handling logic, without any HTTP details. Transport-layer independent, easy to test.

### Constructor

Accepts a `Scorer` and a `MediaLibrary`.

### health() → dict

Returns service status, containing the `ok` flag, algorithm version, model identifier, and media database path.

### score(payload) → dict

Processes a score request. The payload must contain `text` (a non-empty string, up to 2000 characters) and `images` (a list of content hashes, up to 64 items). Images not found in the media database are counted as `missing` rather than raising an error—one missing image should not cause other candidate images for the same word to lose their chance to be scored. Raises `BadRequestError` if the request format is invalid.

## BadRequestError

The request is readable but malformed. Corresponds to HTTP 400, and should not be retried.

## build_handler(service) → Handler class

Builds an HTTP request handler based on the given `ScoreService`. Handles two routes:

- GET / or /health — return health information
- POST /score — read the JSON request body and return the score result

The request body is limited to 1 MB. Logging goes through the `logging` module, controlled by `--quiet`.

## serve(service, host, port)

Starts a multithreaded HTTP service at the specified address, blocking until it receives an interrupt signal.
