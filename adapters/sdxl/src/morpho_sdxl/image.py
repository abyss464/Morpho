"""Render bytes -> the WebP the release budget assumes.

README part 5 budgets images at "WebP 768x576 q80". Quality and encoder method
are pinned here so the same render always produces the same file bytes, and all
metadata is dropped: an ICC profile or EXIF block would make otherwise identical
images hash differently in the content-addressed store.
"""

from __future__ import annotations

import io
import logging
import os
from pathlib import Path
from typing import Final

from morpho_adapter_common import PermanentError, staged_output
from PIL import Image, UnidentifiedImageError

log = logging.getLogger("morpho.adapter.sdxl.image")

QUALITY_ENV: Final[str] = "MORPHO_SDXL_WEBP_QUALITY"
DEFAULT_QUALITY: Final[int] = 80
#: Pillow's slowest, smallest WebP setting. Images are generated once and shipped
#: to every user, so encoder time is free and bytes are not.
WEBP_METHOD: Final[int] = 6

#: SDXL was trained on roughly one megapixel; asking it directly for 768x576
#: renders off-distribution mush. Generate in the nearest native bucket and
#: downscale. Buckets are the standard SDXL set.
NATIVE_BUCKETS: Final[tuple[tuple[int, int], ...]] = (
    (640, 1536),
    (768, 1344),
    (832, 1216),
    (896, 1152),
    (1024, 1024),
    (1152, 896),
    (1216, 832),
    (1344, 768),
    (1536, 640),
)
NATIVE_BUCKETS_ENV: Final[str] = "MORPHO_SDXL_NATIVE_BUCKETS"


def generation_size(width: int, height: int) -> tuple[int, int]:
    """Pick the SDXL bucket to render at before downscaling to `width x height`."""
    if os.environ.get(NATIVE_BUCKETS_ENV, "1").strip().lower() in {"0", "false", "no"}:
        return _round_to_multiple(width, 64), _round_to_multiple(height, 64)
    target = width / height
    best = min(
        NATIVE_BUCKETS,
        # Ties break on area then on the bucket itself: deterministic ordering.
        key=lambda bucket: (abs(bucket[0] / bucket[1] - target), bucket[0] * bucket[1], bucket),
    )
    if best[0] < width or best[1] < height:
        # The caller wants something bigger than any bucket; honour it rather
        # than silently upscaling later.
        return _round_to_multiple(width, 64), _round_to_multiple(height, 64)
    return best


def to_webp(data: bytes, out_path: str | os.PathLike[str], width: int, height: int) -> None:
    """Decode, flatten to RGB, resize to exactly `width x height`, write WebP."""
    try:
        with Image.open(io.BytesIO(data)) as source:
            source.load()
            rgb = source.convert("RGB")
    except UnidentifiedImageError as exc:
        raise PermanentError(f"ComfyUI returned bytes that are not an image: {exc}") from exc
    except OSError as exc:
        raise PermanentError(f"could not decode the render: {exc}") from exc

    if rgb.size != (width, height):
        log.debug("resizing %sx%s -> %sx%s", *rgb.size, width, height)
        rgb = rgb.resize((width, height), Image.Resampling.LANCZOS)

    with staged_output(out_path) as staging:
        rgb.save(
            staging,
            format="WEBP",
            quality=_quality(),
            method=WEBP_METHOD,
            lossless=False,
            exif=b"",
            icc_profile=b"",
        )
    log.info("wrote %sx%s WebP to %s", width, height, Path(out_path).name)


def _quality() -> int:
    raw = os.environ.get(QUALITY_ENV)
    if not raw:
        return DEFAULT_QUALITY
    try:
        value = int(raw)
    except ValueError:
        log.warning("ignoring non-integer %s=%r", QUALITY_ENV, raw)
        return DEFAULT_QUALITY
    return value if 1 <= value <= 100 else DEFAULT_QUALITY


def _round_to_multiple(value: int, multiple: int) -> int:
    return max(multiple, ((value + multiple - 1) // multiple) * multiple)
