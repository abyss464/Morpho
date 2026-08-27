"""Morpho CLIP sidecar: image-text aptness scoring over HTTP.

Runs outside the engine because it needs a GPU the engine's container does not
have, and is reached as a client — the same shape `adapters/sdxl` uses to reach
ComfyUI. See `docs/contracts/clip-service.md`.
"""

from __future__ import annotations

from .media import MediaLibrary, default_root, is_content_hash
from .scorer import ALGO_VER, OpenClipScorer, Scorer
from .service import BadRequestError, ScoreService, build_handler, serve

__all__ = [
    "ALGO_VER",
    "BadRequestError",
    "MediaLibrary",
    "OpenClipScorer",
    "ScoreService",
    "Scorer",
    "build_handler",
    "default_root",
    "is_content_hash",
    "serve",
]
