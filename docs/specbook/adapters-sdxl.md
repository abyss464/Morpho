# adapters/sdxl

7 specs.


============================================================
adapters/sdxl/src/morpho_sdxl/
============================================================


--- __init__.py ---

Package entry point, exports two names:

- **OPS** — a mapping table from operation names to handler functions, for adapters to register and use when processing rows/lines.
- **OP_GENERATE** — the operation name constant `"sdxl.generate"`.


--- __main__.py ---

Running the package's entry point via `python -m morpho_sdxl`. The effect is equivalent to the `sdxl-adapter` console script, directly calling the CLI main function.


--- cli.py ---

# CLI entry point

The entry point module for the console script `sdxl-adapter`.

## main()

Starts the adapter process. Takes the adapter name `"sdxl"` and this adapter's operation table, passes them to the common runtime `morpho_adapter_common.run`, which takes over the message loop. The process exit code is the status code returned by the runtime.


--- comfy.py ---

# ComfyUI HTTP Client

A minimal HTTP client for a local ComfyUI instance: submit workflows, poll for completion, download rendered results. It relies only on the standard library `urllib`, without introducing additional HTTP libraries.

The core design point is error classification. Per the adapter protocol, backend unreachability is a **permanent error** (SDXL is the fallback image source; a machine without ComfyUI won't grow one by retrying), while other failures follow conventional classification.

## ComfyClient

### Creation
Optionally pass in the backend address, transport layer implementation, and client ID. By default, the address is read from the environment variable `COMFYUI_URL`, defaulting to `http://127.0.0.1:8188`.

### probe()
Probe: confirm that something is listening on the backend. Any HTTP response counts as "already configured"; only a connection failure reports "not configured".

### submit(graph) -> prompt_id
Submit a ComfyUI node graph and return the prompt ID.

### await_images(prompt_id, output_node) -> image metadata list
Poll `/history` until the specified prompt completes and return the produced image information. Timeout or execution errors will throw an exception.

### download(image) -> image raw bytes
Download the rendered result's raw bytes from ComfyUI based on the image metadata.

## BackendUnreachableError
A permanent error thrown when ComfyUI is completely unreachable. The error message is fixed as `"sdxl backend not configured"`, conforming to the adapter protocol requirements.

## Environment Variables
- `COMFYUI_URL` — backend address
- `MORPHO_SDXL_TIMEOUT_S` — total timeout waiting for rendering to complete (default 540 seconds)
- `MORPHO_SDXL_POLL_INTERVAL_S` — polling interval (default 1.5 seconds)
- `MORPHO_SDXL_CONNECT_TIMEOUT_S` — connection probe timeout (default 3 seconds)


--- image.py ---

# Image Post-processing

Take the raw bytes rendered by ComfyUI and convert them into the publish spec's WebP file (768×576, q80). Strip all metadata (ICC, EXIF) to ensure that the same render produces the same hash under content-addressed storage.

## generation_size(width, height) -> (width, height)

Select the actual render resolution. SDXL is trained at around one million pixels; directly generating small images will be blurry, so first pick the bucket closest to the target aspect ratio from the native bucket list to render at, then scale to the target size afterward. If the target size is larger than all buckets, align to a multiple of 64 and render directly. Bucket matching can be disabled via the environment variable `MORPHO_SDXL_NATIVE_BUCKETS`.

## to_webp(data, out_path, width, height)

Decode the rendered bytes, convert to RGB, scale to the exact target size, and write out WebP. Use the slowest but smallest encoding settings — the image is generated only once, so file size matters more than encoding time. Write the output atomically via a temporary file.

## Environment Variables

- `MORPHO_SDXL_WEBP_QUALITY` — WebP quality (1–100, default 80)
- `MORPHO_SDXL_NATIVE_BUCKETS` — Set to `0`/`false`/`no` to skip native bucket matching


--- ops.py ---

# sdxl.generate Operation

This is the adapter's only operation. It receives text-to-image parameters, drives ComfyUI rendering, and outputs a WebP file.

## generate(params) -> result dictionary

### Input Parameters
- **prompt** — Positive prompt (required, limit 2000 characters)
- **negative_prompt** — Negative prompt (optional, default `"text, watermark, logo"`, limit 2000 characters)
- **seed** — Random seed (optional, default 0, range 0 to 2^64-1)
- **width** — Output width (optional, default 768, range 64-2048)
- **height** — Output height (optional, default 576, range 64-2048)
- **out_path** — Output file path (required)

### Execution Flow
First check backend liveness, then select the rendering resolution, fill in the workflow template, submit to ComfyUI, poll and wait, download the rendering result, convert to a WebP of the target dimensions, and write to out_path.

### Return Value
Dictionary with two fields:
- **model** — The model label actually used
- **seed** — The seed actually used

## OPS
Mapping table from operation names to handler functions, containing only one item: `"sdxl.generate"` -> `generate`.


--- workflow.py ---

# Workflow Template

Manages loading, validation, and parameter filling of ComfyUI workflow templates. Templates are standard ComfyUI API-format JSON, plus a binding mapping layer declaring which node input each request parameter corresponds to. Template files can be pasted directly into ComfyUI for debugging; they contain no placeholders. Templates are versioned by filename—if the sampler, steps, or cfg changes, use a new file rather than modifying in place.

## WorkflowTemplate

### Properties
- **name** — template filename
- **template_version** — template version number
- **model** — model label (e.g., `"sdxl-base-1.0"`)
- **output_node** — output node ID
- **bindings** — binding table mapping parameter names to (node, field)
- **graph** — ComfyUI node graph

### checkpoint() -> checkpoint name
Reads the default checkpoint name bound in the template.

### render(...) -> node graph
Takes request parameters (prompt, seed, dimensions, etc.) and fills them into a deep copy of the template, returning a node graph that can be submitted directly to ComfyUI.

## load(path) -> WorkflowTemplate
Loads and validates a workflow template from disk. When path is not provided, reads from an environment variable or a default location.

## effective_checkpoint(template) -> checkpoint name
Prefers the value of the environment variable `MORPHO_SDXL_CHECKPOINT`; otherwise uses the template's built-in default checkpoint.

## model_label(template) -> model label
Returns the label written into the `model` field of the response. If operations swapped the checkpoint via an environment variable, use that file's name (minus extension) to ensure the record's provenance isn't misattributed.

## Environment Variables
- `MORPHO_SDXL_WORKFLOW` — custom template file path
- `MORPHO_SDXL_CHECKPOINT` — overrides the template's default checkpoint
