"""Whatever the generator wrote → the WebP the release budget assumes.

Same target and the same pinned encoder settings as `adapters/sdxl`: README
part 5 budgets images at WebP 768x576 q80, and all metadata is dropped because
an ICC profile would make two identical renders hash differently in the
content-addressed store.

The one difference from the SDXL path is the fit. That adapter asks the sampler
for an exact size and gets it; a hosted generator returns whatever aspect ratio
it felt like, so this letterboxes rather than stretching — a squashed photograph
is a worse card than a photograph with a border.
"""

from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import Final

from morpho_adapter_common import PermanentError, staged_output
from PIL import Image, UnidentifiedImageError

log = logging.getLogger("morpho.adapter.codex.image")

QUALITY_ENV: Final[str] = "MORPHO_CODEX_WEBP_QUALITY"
DEFAULT_QUALITY: Final[int] = 80
#: Pillow's slowest, smallest setting. Generated once, shipped to everybody.
WEBP_METHOD: Final[int] = 6
#: What the letterbox is filled with. Black rather than white: a white border
#: reads as part of a diagram, a black one reads as a frame.
LETTERBOX: Final[tuple[int, int, int]] = (0, 0, 0)


def to_webp(source: str | os.PathLike[str], out_path: str | os.PathLike[str],
            width: int, height: int) -> None:
    """Decode, fit into `width x height` preserving aspect, write WebP."""
    try:
        with Image.open(Path(source)) as handle:
            handle.load()
            rgb = handle.convert("RGB")
    except UnidentifiedImageError as exc:
        raise PermanentError(f"codex wrote bytes that are not an image: {exc}") from exc
    except OSError as exc:
        raise PermanentError(f"could not read the generated image: {exc}") from exc

    fitted = _fit(rgb, width, height)
    with staged_output(out_path) as staging:
        fitted.save(
            staging,
            format="WEBP",
            quality=_quality(),
            method=WEBP_METHOD,
            lossless=False,
            exif=b"",
            icc_profile=b"",
        )
    log.info("wrote %sx%s WebP to %s", width, height, Path(out_path).name)


def _fit(image: Image.Image, width: int, height: int) -> Image.Image:
    """Scale to fit inside the box, then centre on a canvas of exactly that size."""
    if image.size == (width, height):
        return image
    scale = min(width / image.width, height / image.height)
    scaled = image.resize(
        (max(1, round(image.width * scale)), max(1, round(image.height * scale))),
        Image.Resampling.LANCZOS,
    )
    if scaled.size == (width, height):
        return scaled
    canvas = Image.new("RGB", (width, height), LETTERBOX)
    canvas.paste(scaled, ((width - scaled.width) // 2, (height - scaled.height) // 2))
    return canvas


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
