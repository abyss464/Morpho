"""Morpho CLIP adapter: image-text aptness scoring.

Two modes:

* **subprocess adapter** (primary) — ``clip-adapter``, spawned by morphod per
  job via the envelope in ``docs/contracts/adapter-protocol.md``.
* **HTTP sidecar** (secondary) — ``clip-sidecar``, a long-running server for
  the GPU path documented in ``docs/contracts/clip-service.md``.
"""

from __future__ import annotations

from .media import MediaLibrary, default_root, is_content_hash
from .ops import OPS
from .scorer import ALGO_VER, OpenClipScorer, Scorer
from .service import BadRequestError, ScoreService, build_handler, serve

__all__ = [
    "ALGO_VER",
    "BadRequestError",
    "MediaLibrary",
    "OPS",
    "OpenClipScorer",
    "ScoreService",
    "Scorer",
    "build_handler",
    "default_root",
    "is_content_hash",
    "serve",
]
