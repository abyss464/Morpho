"""`morfessor.segment` — one process amortised over a whole batch."""

from __future__ import annotations

import logging
from collections.abc import Mapping
from typing import Any, Final

from morpho_adapter_common import PermanentError, require_str_list

from . import model as model_mod

log = logging.getLogger("morpho.adapter.morfessor.ops")

OP_SEGMENT: Final[str] = "morfessor.segment"

#: morphod allows 120 s per batch and ad-hoc training grows with batch size, so
#: refuse absurd batches outright rather than getting killed mid-flight.
MAX_BATCH: Final[int] = 5_000
MAX_WORD_CHARS: Final[int] = 64


def segment(params: Mapping[str, Any]) -> dict[str, Any]:
    words = require_str_list(params, "words")
    if len(words) > MAX_BATCH:
        raise PermanentError(f"batch of {len(words)} words exceeds the {MAX_BATCH} word limit")

    cleaned = _clean(words)
    result = model_mod.segment_batch(cleaned)
    log.info(
        "segmented %d words via %s (%s)",
        len(result.segments),
        result.source,
        result.model_ver,
    )
    return {"segments": result.segments, "model_ver": result.model_ver}


def _clean(words: list[str]) -> list[str]:
    """Strip surrounding whitespace, keep case and first-seen order, drop duplicates."""
    seen: dict[str, None] = {}
    for raw in words:
        word = raw.strip()
        if not word:
            raise PermanentError("param 'words' contains a whitespace-only entry")
        if len(word) > MAX_WORD_CHARS:
            raise PermanentError(
                f"word {word[:32]!r}... exceeds {MAX_WORD_CHARS} characters ({len(word)})"
            )
        seen.setdefault(word, None)
    return list(seen)


OPS: Final[dict[str, Any]] = {OP_SEGMENT: segment}
