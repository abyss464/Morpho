"""``clip.score`` — score images against a text query via CLIP.

This is the subprocess adapter op, following the envelope in
``docs/contracts/adapter-protocol.md``.  The engine sends a list of content
hashes and the path to its own media library, and gets back cosines and an
identity.

The model is loaded on every invocation (one subprocess per job), which is the
known cost of the subprocess pattern.  On a GPU host the load is a few seconds;
on CPU it is longer, and the choice is accepted because it trades speed for
operational reliability — the sidecar that held the model in memory went down on
every reboot and left thousands of images unscored.
"""

from __future__ import annotations

import logging
import os
from collections.abc import Mapping
from typing import Any, Final

from morpho_adapter_common import PermanentError, require_str, require_str_list

from .media import MediaLibrary
from .scorer import ALGO_VER, DEFAULT_ARCH, DEFAULT_PRETRAINED, OpenClipScorer

log = logging.getLogger("morpho.adapter.clip.ops")

OP_SCORE: Final[str] = "clip.score"

#: Mirrors ``morpho_reconcile::sources::clip::MAX_IMAGES_PER_REQUEST``.
MAX_IMAGES: Final[int] = 64
#: A sentence, not an essay.
MAX_TEXT_CHARS: Final[int] = 2_000

# Environment overrides — same knobs the sidecar CLI exposes as flags.
_ARCH_ENV: Final[str] = "MORPHO_CLIP_ARCH"
_PRETRAINED_ENV: Final[str] = "MORPHO_CLIP_PRETRAINED"
_DEVICE_ENV: Final[str] = "MORPHO_CLIP_DEVICE"


def _env(name: str) -> str | None:
    value = os.environ.get(name, "").strip()
    return value or None


def score(params: Mapping[str, Any]) -> dict[str, Any]:
    """Score a word's pictures against a text query.

    Params::

        text       — the slot-1 sentence (or lemma), scored verbatim
        images     — content hashes of the pictures to score (may be empty)
        media_root — absolute path to the content-addressed media library

    Result::

        model    — ``<arch>/<pretrained>``
        algo_ver — algorithm version (``clip/1``)
        scores   — ``[{file_hash, similarity}, ...]``
        missing  — hashes the library could not produce
    """
    text = require_str(params, "text")
    if len(text) > MAX_TEXT_CHARS:
        raise PermanentError(f"param 'text' exceeds {MAX_TEXT_CHARS} characters")

    images = require_str_list(params, "images", allow_empty=True)
    media_root = require_str(params, "media_root")

    if len(images) > MAX_IMAGES:
        raise PermanentError(f"param 'images' exceeds {MAX_IMAGES} entries")

    arch = _env(_ARCH_ENV) or DEFAULT_ARCH
    pretrained = _env(_PRETRAINED_ENV) or DEFAULT_PRETRAINED

    library = MediaLibrary(media_root)

    resolved: list[tuple[str, Any]] = []
    missing: list[str] = []
    for file_hash in images:
        path = library.path_for(file_hash)
        if path is None:
            missing.append(file_hash)
        else:
            resolved.append((file_hash, path))

    # No pictures to score — report the identity without loading the model.
    if not resolved:
        return {
            "model": f"{arch}/{pretrained}",
            "algo_ver": ALGO_VER,
            "scores": [],
            "missing": missing,
        }

    scorer = OpenClipScorer(
        arch=arch,
        pretrained=pretrained,
        device=_env(_DEVICE_ENV),
    )

    similarities = scorer.score(text, [path for _, path in resolved])
    if len(similarities) != len(resolved):
        raise RuntimeError(
            f"scorer returned {len(similarities)} scores for {len(resolved)} pictures"
        )

    return {
        "model": scorer.model,
        "algo_ver": ALGO_VER,
        "scores": [
            {"file_hash": fh, "similarity": sim}
            for (fh, _), sim in zip(resolved, similarities, strict=True)
        ],
        "missing": missing,
    }


OPS: Final[dict[str, Any]] = {OP_SCORE: score}
