"""edge-tts driver and its failure classification.

Classification is deliberately name- and message-driven rather than built on
`isinstance` against `edge_tts.exceptions`: those classes get renamed between
minor releases, and a misclassified failure is worse than a slightly fuzzy
match — `permanent` silently drops a word, `transient` burns the retry budget.
Everything here is a pure function over an exception object, so the whole
mapping is unit-testable without touching the network.
"""

from __future__ import annotations

import asyncio
import logging
import os
import re
from importlib import metadata
from typing import Any, Final

import edge_tts
from morpho_adapter_common import (
    DEFAULT_RATE_LIMIT_MS,
    AdapterError,
    PermanentError,
    RateLimitedError,
    TransientError,
)

log = logging.getLogger("morpho.adapter.tts.engine")

TIMEOUT_ENV: Final[str] = "MORPHO_TTS_TIMEOUT_S"
#: morphod kills the adapter at 60 s; bail out first so the failure comes back
#: as a classified response instead of an opaque kill.
DEFAULT_TIMEOUT_S: Final[float] = 45.0

RATE_PATTERN: Final[re.Pattern[str]] = re.compile(r"^[+-]\d+%$")
PITCH_PATTERN: Final[re.Pattern[str]] = re.compile(r"^[+-]\d+Hz$")
VOLUME_PATTERN: Final[re.Pattern[str]] = re.compile(r"^[+-]\d+%$")

_RATE_LIMIT_MARKERS: Final[tuple[str, ...]] = (
    "429",
    "too many requests",
    "rate limit",
    "ratelimit",
    "throttl",
    "quota exceeded",
)

#: Names that mean "this exact request will never succeed".
_PERMANENT_NAMES: Final[frozenset[str]] = frozenset(
    {
        "NoAudioReceived",
        "ValueError",
        "TypeError",
        "UnicodeEncodeError",
        "UnicodeDecodeError",
    }
)

#: Names that mean "the network or the far side misbehaved".
_TRANSIENT_NAMES: Final[frozenset[str]] = frozenset(
    {
        "ClientConnectorError",
        "ClientError",
        "ClientOSError",
        "ClientResponseError",
        "ConnectionClosed",
        "ConnectionClosedError",
        "ConnectionError",
        "ConnectionRefusedError",
        "ConnectionResetError",
        "InvalidStatus",
        "InvalidStatusCode",
        "OSError",
        "ServerDisconnectedError",
        "ServerTimeoutError",
        "SkewAdjustmentError",
        "TimeoutError",
        "UnexpectedResponse",
        "UnknownResponse",
        "WebSocketError",
        "WebSocketException",
        "gaierror",
    }
)

_STATUS_PATTERN: Final[re.Pattern[str]] = re.compile(r"(?<!\d)([45]\d{2})(?!\d)")
_STATUS_ATTRS: Final[tuple[str, ...]] = ("status", "status_code", "code", "response_status")


def engine_version() -> str:
    """`edge-tts/<version>` — goes straight into the TTS row's `engine_ver`."""
    try:
        return f"edge-tts/{metadata.version('edge-tts')}"
    except metadata.PackageNotFoundError:  # pragma: no cover - dependency is declared
        return "edge-tts/unknown"


def validate_prosody(rate: str, pitch: str, volume: str) -> None:
    """Reject malformed prosody locally so we never spend a network round trip."""
    if not RATE_PATTERN.match(rate):
        raise PermanentError(f"param 'rate' must look like '+0%' or '-10%', got {rate!r}")
    if not PITCH_PATTERN.match(pitch):
        raise PermanentError(f"param 'pitch' must look like '+0Hz' or '-5Hz', got {pitch!r}")
    if not VOLUME_PATTERN.match(volume):
        raise PermanentError(f"param 'volume' must look like '+0%' or '-10%', got {volume!r}")


def synthesize_mp3(
    text: str,
    voice: str,
    *,
    rate: str = "+0%",
    pitch: str = "+0Hz",
    volume: str = "+0%",
    timeout_s: float | None = None,
) -> bytes:
    """Return the raw mp3 bytes edge-tts streams back, or raise an `AdapterError`."""
    budget = timeout_s if timeout_s is not None else timeout_from_env()
    try:
        audio = asyncio.run(_stream(text, voice, rate, pitch, volume, budget))
    except AdapterError:
        raise
    except BaseException as exc:
        if isinstance(exc, KeyboardInterrupt | SystemExit):
            raise
        raise classify(exc) from exc
    if not audio:
        raise PermanentError(
            "edge-tts returned no audio; check that the voice exists and the text is speakable"
        )
    log.debug("edge-tts produced %d mp3 bytes", len(audio))
    return audio


async def _stream(
    text: str, voice: str, rate: str, pitch: str, volume: str, timeout_s: float
) -> bytes:
    chunks: list[bytes] = []
    async with asyncio.timeout(timeout_s):
        communicate = edge_tts.Communicate(
            text=text, voice=voice, rate=rate, pitch=pitch, volume=volume
        )
        async for chunk in communicate.stream():
            if chunk.get("type") == "audio" and chunk.get("data"):
                chunks.append(chunk["data"])
    return b"".join(chunks)


def timeout_from_env() -> float:
    raw = os.environ.get(TIMEOUT_ENV)
    if not raw:
        return DEFAULT_TIMEOUT_S
    try:
        value = float(raw)
    except ValueError:
        log.warning("ignoring non-numeric %s=%r", TIMEOUT_ENV, raw)
        return DEFAULT_TIMEOUT_S
    return value if value > 0 else DEFAULT_TIMEOUT_S


# --- classification ---------------------------------------------------------


def classify(exc: BaseException) -> AdapterError:
    """Map an edge-tts / asyncio / socket failure onto the wire taxonomy."""
    chain = _chain(exc)
    names = {type(item).__name__ for item in chain}
    blob = " | ".join(f"{type(item).__name__}: {item}" for item in chain)
    lowered = blob.lower()
    summary = _summarize(exc, blob)

    if any(marker in lowered for marker in _RATE_LIMIT_MARKERS):
        return RateLimitedError(
            f"edge-tts rate limited: {summary}", retry_after_ms=_retry_after(blob)
        )

    status = _http_status(chain, blob)
    if status is not None:
        if status == 429:
            return RateLimitedError(
                f"edge-tts rate limited (HTTP {status}): {summary}",
                retry_after_ms=_retry_after(blob),
            )
        if status in {401, 403, 408}:
            # Edge's anonymous endpoint hands out 401/403 when its token scheme
            # drifts. It clears up on its own often enough to be worth backoff;
            # the retry cap turns a genuine outage into a dead letter anyway.
            return TransientError(f"edge-tts refused the request (HTTP {status}): {summary}")
        if 400 <= status < 500:
            return PermanentError(f"edge-tts rejected the request (HTTP {status}): {summary}")
        return TransientError(f"edge-tts upstream error (HTTP {status}): {summary}")

    if names & _PERMANENT_NAMES:
        return PermanentError(f"edge-tts cannot serve this request: {summary}")
    if names & _TRANSIENT_NAMES:
        return TransientError(f"edge-tts call failed: {summary}")
    if isinstance(exc, asyncio.TimeoutError | TimeoutError):
        return TransientError(f"edge-tts timed out: {summary}")
    return TransientError(f"edge-tts call failed ({type(exc).__name__}): {summary}")


def _chain(exc: BaseException, limit: int = 8) -> list[BaseException]:
    seen: list[BaseException] = []
    current: BaseException | None = exc
    while current is not None and len(seen) < limit:
        if any(current is item for item in seen):
            break
        seen.append(current)
        current = current.__cause__ or current.__context__
    return seen


def _http_status(chain: list[BaseException], blob: str) -> int | None:
    for item in chain:
        for attr in _STATUS_ATTRS:
            value = getattr(item, attr, None)
            if isinstance(value, int) and 100 <= value < 600:
                return value
        response = getattr(item, "response", None)
        if response is not None:
            for attr in _STATUS_ATTRS:
                value = getattr(response, attr, None)
                if isinstance(value, int) and 100 <= value < 600:
                    return value
    match = _STATUS_PATTERN.search(blob)
    return int(match.group(1)) if match else None


def _retry_after(blob: str) -> int:
    match = re.search(r"retry[-_ ]?after[\"'\s:=]+(\d+)", blob, re.IGNORECASE)
    if not match:
        return DEFAULT_RATE_LIMIT_MS
    seconds = int(match.group(1))
    return max(1, min(seconds, 3600)) * 1000


def _summarize(exc: BaseException, blob: str, limit: int = 300) -> str:
    text = str(exc).strip() or blob.strip() or type(exc).__name__
    text = " ".join(text.split())
    return text if len(text) <= limit else text[: limit - 1] + "…"


def voice_catalog() -> list[dict[str, Any]]:  # pragma: no cover - network helper
    """Live voice list. Operator convenience only; morphod never calls this."""
    return asyncio.run(edge_tts.list_voices())
