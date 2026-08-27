"""The blank-image gate, ported from `ops/verify_genimg.py`.

A generator that refuses on content policy, or runs out of quota mid-render,
does not always fail — sometimes it writes a solid white canvas. Those images
are worth nothing and cost everything downstream: they enter the library, take a
content address, get scored, and sit in a slot until somebody notices a word
whose picture is a blank rectangle.

Grayscale pixel standard deviation catches them for the price of a decode.
Anything genuinely photographic is far above the threshold; a flat colour field
is at zero.

The engine's own CLIP floor catches the *other* failure — a real picture of the
wrong thing — so this only has to catch the pictures that are not pictures.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Final

from PIL import Image, ImageStat, UnidentifiedImageError

#: `ops/verify_genimg.py`'s `BLANK_STDDEV_THRESHOLD`, unchanged. On a 0-255
#: scale this is very nearly zero: it rejects a uniform field and nothing else.
DEFAULT_BLANK_STDDEV: Final[float] = 0.5
BLANK_STDDEV_ENV: Final[str] = "MORPHO_CODEX_BLANK_STDDEV"


def blank_threshold() -> float:
    raw = os.environ.get(BLANK_STDDEV_ENV, "").strip()
    if not raw:
        return DEFAULT_BLANK_STDDEV
    try:
        return float(raw)
    except ValueError:
        return DEFAULT_BLANK_STDDEV


def is_blank(path: str | os.PathLike[str], threshold: float | None = None) -> bool:
    """Is this image a flat colour field rather than a picture?

    An unreadable file counts as blank: either way there is nothing here worth
    putting in front of a learner, and the caller's answer to both is the same.
    """
    limit = blank_threshold() if threshold is None else threshold
    try:
        with Image.open(Path(path)) as handle:
            grey = handle.convert("L")
            stddev = ImageStat.Stat(grey).stddev[0]
    except (UnidentifiedImageError, OSError, ValueError):
        return True
    return stddev < limit
