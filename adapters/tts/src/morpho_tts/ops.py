"""`tts.synthesize` — the only op this adapter serves."""

from __future__ import annotations

import logging
import tempfile
from collections.abc import Mapping
from pathlib import Path
from typing import Any, Final

from morpho_adapter_common import (
    PermanentError,
    require_choice,
    require_int,
    require_out_path,
    require_str,
    staged_output,
)

from . import engine, oggopus, transcode

log = logging.getLogger("morpho.adapter.tts.ops")

OP_SYNTHESIZE: Final[str] = "tts.synthesize"

SUPPORTED_FORMATS: Final[tuple[str, ...]] = ("ogg_opus",)
DEFAULT_FORMAT: Final[str] = "ogg_opus"
DEFAULT_BITRATE_KBPS: Final[int] = 32
MIN_BITRATE_KBPS: Final[int] = 6
MAX_BITRATE_KBPS: Final[int] = 510
#: Definitions and examples are one sentence; anything longer is a caller bug,
#: and Edge's endpoint would truncate it silently rather than fail loudly.
MAX_TEXT_CHARS: Final[int] = 5_000


def synthesize(params: Mapping[str, Any]) -> dict[str, Any]:
    text = require_str(params, "text")
    voice = require_str(params, "voice")
    rate = require_str(params, "rate", default="+0%")
    pitch = require_str(params, "pitch", default="+0Hz")
    volume = require_str(params, "volume", default="+0%")
    require_choice(params, "format", SUPPORTED_FORMATS, default=DEFAULT_FORMAT)
    bitrate_kbps = require_int(
        params,
        "bitrate_kbps",
        default=DEFAULT_BITRATE_KBPS,
        minimum=MIN_BITRATE_KBPS,
        maximum=MAX_BITRATE_KBPS,
    )
    out_path = require_out_path(params)

    if len(text) > MAX_TEXT_CHARS:
        raise PermanentError(f"param 'text' exceeds {MAX_TEXT_CHARS} characters ({len(text)})")
    engine.validate_prosody(rate, pitch, volume)

    # Resolve ffmpeg before the network call: a missing transcoder makes the
    # whole job permanently impossible, and there is no point paying for
    # synthesis first.
    ffmpeg = transcode.find_ffmpeg()

    mp3 = engine.synthesize_mp3(text, voice, rate=rate, pitch=pitch, volume=volume)

    with tempfile.TemporaryDirectory(prefix="morpho-tts-") as scratch:
        source = Path(scratch) / "edge.mp3"
        source.write_bytes(mp3)
        with staged_output(out_path) as staging:
            transcode.to_ogg_opus(source, staging, bitrate_kbps, ffmpeg=ffmpeg)
            info = _probe(staging)

    log.info(
        "synthesized %d chars with %s -> %d ms, %d kbps mono opus",
        len(text),
        voice,
        info.duration_ms,
        bitrate_kbps,
    )
    return {"duration_ms": info.duration_ms, "engine_ver": engine.engine_version()}


def _probe(path: Path) -> oggopus.OpusInfo:
    try:
        return oggopus.parse_file(path)
    except oggopus.OggParseError as exc:
        raise PermanentError(f"ffmpeg produced an unreadable Ogg Opus file: {exc}") from exc


OPS: Final[dict[str, Any]] = {OP_SYNTHESIZE: synthesize}
