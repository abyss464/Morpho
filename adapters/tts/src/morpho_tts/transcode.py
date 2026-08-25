"""mp3 -> mono Ogg Opus via ffmpeg, with settings pinned for byte stability.

The contract asks for idempotent output: the same request must yield the same
bytes so morphod's content-addressed store deduplicates instead of churning.
Every knob that would otherwise leak into the file is nailed down here —
metadata stripped, encoder/vendor tags suppressed via bitexact, sample rate and
channel count forced rather than inherited from whatever edge-tts returned.
"""

from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import Final

from morpho_adapter_common import PermanentError, resolve_binary, run_binary

log = logging.getLogger("morpho.adapter.tts.transcode")

FFMPEG_ENV: Final[str] = "MORPHO_FFMPEG"
FFMPEG_TIMEOUT_ENV: Final[str] = "MORPHO_TTS_FFMPEG_TIMEOUT_S"
DEFAULT_FFMPEG_TIMEOUT_S: Final[float] = 30.0

#: Bumping any pinned flag below changes the output bytes. morphod mixes the
#: engine version into the TTS `input_hash`; report this alongside it so a
#: transcoder change invalidates cleanly instead of silently drifting.
TRANSCODER_VER: Final[str] = "ffmpeg-libopus/1"

_MISSING_ENCODER_MARKERS: Final[tuple[str, ...]] = (
    "unknown encoder",
    "encoder not found",
    "libopus",
)


def find_ffmpeg() -> str:
    """Locate ffmpeg. Absent ffmpeg is permanent: retrying will not install it."""
    return resolve_binary(
        "ffmpeg",
        env_var=FFMPEG_ENV,
        hint=f"required to transcode TTS output to Ogg Opus; set {FFMPEG_ENV} to override",
    )


def build_argv(ffmpeg: str, src: Path, dst: Path, bitrate_kbps: int) -> list[str]:
    return [
        ffmpeg,
        "-nostdin",
        "-hide_banner",
        "-nostats",
        "-loglevel",
        "error",
        "-y",
        "-fflags",
        "+bitexact",
        "-i",
        str(src),
        "-map",
        "0:a:0",
        "-map_metadata",
        "-1",
        "-map_chapters",
        "-1",
        "-vn",
        "-sn",
        "-dn",
        "-ac",
        "1",
        "-ar",
        "48000",
        "-c:a",
        "libopus",
        "-b:a",
        f"{bitrate_kbps}k",
        "-vbr",
        "on",
        "-application",
        "audio",
        "-frame_duration",
        "20",
        "-compression_level",
        "10",
        "-flags",
        "+bitexact",
        "-fflags",
        "+bitexact",
        "-f",
        "ogg",
        str(dst),
    ]


def to_ogg_opus(src: Path, dst: Path, bitrate_kbps: int, *, ffmpeg: str | None = None) -> None:
    """Transcode `src` (mp3) onto `dst` (Ogg Opus, mono, 48 kHz)."""
    binary = ffmpeg or find_ffmpeg()
    argv = build_argv(binary, src, dst, bitrate_kbps)
    log.debug("transcoding %s -> %s at %d kbps", src, dst, bitrate_kbps)
    try:
        run_binary(argv, timeout_s=_timeout_s(), what="ffmpeg")
    except PermanentError as exc:
        lowered = exc.message.lower()
        if any(marker in lowered for marker in _MISSING_ENCODER_MARKERS):
            raise PermanentError(
                "this ffmpeg build has no libopus encoder; install ffmpeg with "
                f"--enable-libopus (original error: {exc.message})"
            ) from exc
        raise
    if not dst.exists() or dst.stat().st_size == 0:
        raise PermanentError("ffmpeg reported success but produced no audio")


def _timeout_s() -> float:
    raw = os.environ.get(FFMPEG_TIMEOUT_ENV)
    if not raw:
        return DEFAULT_FFMPEG_TIMEOUT_S
    try:
        value = float(raw)
    except ValueError:
        log.warning("ignoring non-numeric %s=%r", FFMPEG_TIMEOUT_ENV, raw)
        return DEFAULT_FFMPEG_TIMEOUT_S
    return value if value > 0 else DEFAULT_FFMPEG_TIMEOUT_S
