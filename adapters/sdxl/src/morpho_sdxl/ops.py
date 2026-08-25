"""`sdxl.generate` — the only op this adapter serves."""

from __future__ import annotations

import logging
from collections.abc import Mapping
from typing import Any, Final

from morpho_adapter_common import (
    PermanentError,
    require_int,
    require_out_path,
    require_str,
)

from . import comfy, image, workflow

log = logging.getLogger("morpho.adapter.sdxl.ops")

OP_GENERATE: Final[str] = "sdxl.generate"

DEFAULT_NEGATIVE_PROMPT: Final[str] = "text, watermark, logo"
DEFAULT_WIDTH: Final[int] = 768
DEFAULT_HEIGHT: Final[int] = 576
MIN_DIMENSION: Final[int] = 64
MAX_DIMENSION: Final[int] = 2048
#: ComfyUI seeds are unsigned 64-bit.
MAX_SEED: Final[int] = 2**64 - 1
MAX_PROMPT_CHARS: Final[int] = 2_000


def generate(params: Mapping[str, Any]) -> dict[str, Any]:
    prompt = require_str(params, "prompt")
    negative_prompt = require_str(
        params, "negative_prompt", default=DEFAULT_NEGATIVE_PROMPT, allow_empty=True
    )
    seed = require_int(params, "seed", default=0, minimum=0, maximum=MAX_SEED)
    width = require_int(
        params, "width", default=DEFAULT_WIDTH, minimum=MIN_DIMENSION, maximum=MAX_DIMENSION
    )
    height = require_int(
        params, "height", default=DEFAULT_HEIGHT, minimum=MIN_DIMENSION, maximum=MAX_DIMENSION
    )
    out_path = require_out_path(params)
    _check_prompt_length(prompt, "prompt")
    _check_prompt_length(negative_prompt, "negative_prompt")

    template = workflow.load()
    client = comfy.ComfyClient()
    # Probe first: an absent backend must come back as the contract's permanent
    # "sdxl backend not configured" in milliseconds, not after a render timeout.
    client.probe()

    render_width, render_height = image.generation_size(width, height)
    graph = template.render(
        prompt=prompt,
        negative_prompt=negative_prompt,
        seed=seed,
        width=render_width,
        height=render_height,
        checkpoint=workflow.effective_checkpoint(template),
    )
    log.info(
        "generating %dx%d (rendering %dx%d) with %s seed %d",
        width,
        height,
        render_width,
        render_height,
        template.name,
        seed,
    )

    prompt_id = client.submit(graph)
    images = client.await_images(prompt_id, template.output_node)
    data = client.download(images[0])
    image.to_webp(data, out_path, width, height)

    return {"model": workflow.model_label(template), "seed": seed}


def _check_prompt_length(text: str, name: str) -> None:
    if len(text) > MAX_PROMPT_CHARS:
        raise PermanentError(f"param {name!r} exceeds {MAX_PROMPT_CHARS} characters ({len(text)})")


OPS: Final[dict[str, Any]] = {OP_GENERATE: generate}
