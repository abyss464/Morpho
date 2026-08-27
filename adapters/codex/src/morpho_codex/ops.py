"""`codex.generate` — the only op this adapter serves.

Four steps, and three of them are gates:

1. build the prompt for the requested template version;
2. run the generator, which writes a file of its own choosing next to the
   staging path;
3. refuse a blank canvas (`ops/verify_genimg.py`'s gate, ported verbatim in
   `quality`);
4. fit and re-encode onto the `out_path` morphod owns.

Everything the *engine* can check — is this picture apt, is it better than the
one the word already has, is it somebody else's — the engine checks, because it
has CLIP and a database and this has neither. What is left here is the pair of
questions that can only be answered next to the file: did anything get written,
and is what got written a picture at all.
"""

from __future__ import annotations

import contextlib
import logging
from collections.abc import Mapping
from pathlib import Path
from typing import Any, Final

from morpho_adapter_common import (
    PermanentError,
    require_int,
    require_out_path,
    require_str,
)

from . import image, prompt, quality, runner

log = logging.getLogger("morpho.adapter.codex.ops")

OP_GENERATE: Final[str] = "codex.generate"

DEFAULT_WIDTH: Final[int] = 768
DEFAULT_HEIGHT: Final[int] = 576
MIN_DIMENSION: Final[int] = 64
MAX_DIMENSION: Final[int] = 2048
MAX_TEXT_CHARS: Final[int] = 2_000

#: What the generator is told to write, relative to the staging directory. It is
#: not `out_path` itself: morphod hashes whatever lands there, and a blank canvas
#: that failed the gate must never have been at that path even for an instant.
RENDER_NAME: Final[str] = "render.png"


def generate(params: Mapping[str, Any]) -> dict[str, Any]:
    lemma = require_str(params, "lemma")
    prompt_ver = require_str(params, "prompt_ver", default=prompt.CURRENT_VERSION)
    if prompt_ver != prompt.CURRENT_VERSION:
        # A candidate's `source_ref` records the version that drew it. Drawing
        # under a different one than the caller asked for would make that record
        # a lie, and nothing downstream could tell.
        raise PermanentError(
            f"this adapter implements prompt {prompt.CURRENT_VERSION}, not {prompt_ver!r}"
        )
    pos = _optional(params, "pos")
    definition = _optional(params, "primary_definition")
    # Required, and the one required field besides the lemma. This source draws
    # the scene a sentence describes; the engine defers a word that has none
    # rather than asking for one, so a request without it is a caller bug rather
    # than a word early in its life.
    sentence = require_str(params, "slot1_sentence")
    if len(sentence) > MAX_TEXT_CHARS:
        raise PermanentError(f"param 'slot1_sentence' exceeds {MAX_TEXT_CHARS} characters")
    width = require_int(
        params, "width", default=DEFAULT_WIDTH, minimum=MIN_DIMENSION, maximum=MAX_DIMENSION
    )
    height = require_int(
        params, "height", default=DEFAULT_HEIGHT, minimum=MIN_DIMENSION, maximum=MAX_DIMENSION
    )
    out_path = Path(require_out_path(params))

    workdir = out_path.parent
    render = workdir / RENDER_NAME
    _unlink_quietly(render)

    text = prompt.build(
        lemma=lemma,
        out_path=str(render),
        width=width,
        height=height,
        pos=pos,
        primary_definition=definition,
        slot1_sentence=sentence,
    )
    model = runner.generate(text, workdir)

    if not render.is_file():
        # An exit of zero with nothing written is the ordinary shape of a
        # content-policy refusal. Permanent: the same prompt gets the same
        # refusal, and the word belongs in dead letters where somebody can see
        # which words the generator will not draw.
        raise PermanentError(f"codex wrote no image for {lemma!r}")
    if quality.is_blank(render):
        raise PermanentError(
            f"codex produced a blank canvas for {lemma!r} "
            f"(grayscale stddev below {quality.blank_threshold()})"
        )

    image.to_webp(render, out_path, width, height)
    _unlink_quietly(render)
    return {"model": model, "prompt": text}


def _optional(params: Mapping[str, Any], name: str) -> str | None:
    """A string parameter that may legitimately be absent or empty.

    A word with no sentence, no recorded part of speech or no selected
    definition is an ordinary word early in its life, not a bad request — the
    prompt simply says less about it.
    """
    value = params.get(name)
    if value is None:
        return None
    if not isinstance(value, str):
        raise PermanentError(f"param {name!r} must be a string")
    value = value.strip()
    if len(value) > MAX_TEXT_CHARS:
        raise PermanentError(f"param {name!r} exceeds {MAX_TEXT_CHARS} characters")
    return value or None


def _unlink_quietly(path: Path) -> None:
    with contextlib.suppress(FileNotFoundError, IsADirectoryError, PermissionError):
        path.unlink()


OPS: Final[dict[str, Any]] = {OP_GENERATE: generate}
