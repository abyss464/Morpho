"""edge-tts driving and failure classification — entirely offline."""

from __future__ import annotations

import asyncio

import pytest
from morpho_adapter_common import PermanentError, RateLimitedError, TransientError

from morpho_tts import engine

# --- driving ----------------------------------------------------------------


def test_synthesize_collects_audio_chunks_only(fake_edge_tts) -> None:
    audio = engine.synthesize_mp3("hello", "en-US-AriaNeural")
    assert audio == b"\xff\xfb\x90payload-mp3"
    call = fake_edge_tts.instances[-1]
    assert (call.text, call.voice, call.rate, call.pitch, call.volume) == (
        "hello",
        "en-US-AriaNeural",
        "+0%",
        "+0Hz",
        "+0%",
    )


def test_prosody_is_forwarded(fake_edge_tts) -> None:
    engine.synthesize_mp3("hi", "en-GB-RyanNeural", rate="-10%", pitch="+5Hz", volume="-20%")
    call = fake_edge_tts.instances[-1]
    assert (call.rate, call.pitch, call.volume) == ("-10%", "+5Hz", "-20%")


def test_empty_audio_stream_is_permanent(fake_edge_tts) -> None:
    fake_edge_tts.audio_chunks = ()
    with pytest.raises(PermanentError, match="no audio"):
        engine.synthesize_mp3("hello", "en-US-AriaNeural")


def test_engine_version_is_reported() -> None:
    version = engine.engine_version()
    assert version.startswith("edge-tts/")
    assert version != "edge-tts/unknown"


@pytest.mark.parametrize(
    ("rate", "pitch", "volume"),
    [("+0%", "+0Hz", "+0%"), ("-25%", "-10Hz", "+50%"), ("+100%", "+200Hz", "-100%")],
)
def test_valid_prosody_accepted(rate: str, pitch: str, volume: str) -> None:
    engine.validate_prosody(rate, pitch, volume)


@pytest.mark.parametrize(
    ("rate", "pitch", "volume"),
    [
        ("0%", "+0Hz", "+0%"),
        ("+0", "+0Hz", "+0%"),
        ("fast", "+0Hz", "+0%"),
        ("+0%", "+0", "+0%"),
        ("+0%", "+0hz", "+0%"),
        ("+0%", "+0Hz", "loud"),
    ],
)
def test_invalid_prosody_is_permanent(rate: str, pitch: str, volume: str) -> None:
    with pytest.raises(PermanentError):
        engine.validate_prosody(rate, pitch, volume)


# --- classification ---------------------------------------------------------


def _named(name: str, message: str, **attrs: object) -> BaseException:
    """Build an exception whose *class name* drives classification."""
    cls = type(name, (Exception,), {})
    exc = cls(message)
    for key, value in attrs.items():
        setattr(exc, key, value)
    return exc


@pytest.mark.parametrize(
    "exc",
    [
        _named("NoAudioReceived", "No audio was received. Please verify your parameters."),
        ValueError("Invalid rate 'quick'."),
        TypeError("text must be str"),
        _named("SomeError", "server said 400 Bad Request"),
        _named("InvalidStatus", "server rejected WebSocket connection", status=404),
    ],
)
def test_permanent_classifications(exc: BaseException) -> None:
    assert engine.classify(exc).kind == "permanent"


@pytest.mark.parametrize(
    "exc",
    [
        _named("WebSocketError", "connection closed abnormally"),
        _named("ClientConnectorError", "Cannot connect to host speech.platform.bing.com"),
        ConnectionResetError("Connection reset by peer"),
        TimeoutError("timed out"),
        TimeoutError(),
        OSError("network is unreachable"),
        _named("UnknownResponse", "unexpected frame"),
        _named("InvalidStatus", "server rejected WebSocket connection", status=503),
        _named("InvalidStatus", "server rejected WebSocket connection", status=403),
        _named("SomeError", "HTTP 500 Internal Server Error"),
        _named("BrandNewEdgeTtsError", "something we have never seen"),
    ],
)
def test_transient_classifications(exc: BaseException) -> None:
    assert engine.classify(exc).kind == "transient"


@pytest.mark.parametrize(
    "exc",
    [
        _named("WebSocketError", "server rejected WebSocket connection: HTTP 429"),
        _named("ClientResponseError", "Too Many Requests", status=429),
        _named("EdgeError", "rate limit exceeded, slow down"),
        _named("EdgeError", "request was throttled"),
    ],
)
def test_rate_limited_classifications(exc: BaseException) -> None:
    error = engine.classify(exc)
    assert error.kind == "rate_limited"
    assert error.retry_after_ms > 0


def test_rate_limit_honors_retry_after_header() -> None:
    exc = _named("ClientResponseError", "429 Too Many Requests, Retry-After: 30")
    error = engine.classify(exc)
    assert isinstance(error, RateLimitedError)
    assert error.retry_after_ms == 30_000


def test_rate_limit_falls_back_to_a_default_cooldown() -> None:
    error = engine.classify(_named("EdgeError", "rate limit exceeded"))
    assert error.retry_after_ms == 60_000


def test_status_is_read_through_a_nested_response_object() -> None:
    response = type("Response", (), {"status_code": 429})()
    exc = _named("InvalidStatus", "rejected", response=response)
    assert engine.classify(exc).kind == "rate_limited"


def test_classification_walks_the_cause_chain() -> None:
    root = ConnectionRefusedError("connection refused")
    wrapper = _named("WebSocketError", "handshake failed")
    wrapper.__cause__ = root
    assert engine.classify(wrapper).kind == "transient"


def test_classification_survives_a_cyclic_chain() -> None:
    first = _named("A", "one")
    second = _named("B", "two")
    first.__cause__ = second
    second.__cause__ = first
    assert engine.classify(first).kind == "transient"


def test_synthesize_maps_engine_failures(fake_edge_tts) -> None:
    fake_edge_tts.raises = _named("WebSocketError", "HTTP 429 Too Many Requests")
    with pytest.raises(RateLimitedError):
        engine.synthesize_mp3("hello", "en-US-AriaNeural")

    fake_edge_tts.raises = _named("NoAudioReceived", "No audio was received.")
    with pytest.raises(PermanentError):
        engine.synthesize_mp3("hello", "en-US-BogusVoice")

    fake_edge_tts.raises = ConnectionResetError("reset")
    with pytest.raises(TransientError):
        engine.synthesize_mp3("hello", "en-US-AriaNeural")


def test_timeout_is_transient_and_bounded(fake_edge_tts, monkeypatch: pytest.MonkeyPatch) -> None:
    class SlowCommunicate(fake_edge_tts):  # type: ignore[misc, valid-type]
        async def stream(self):  # type: ignore[no-untyped-def]
            await asyncio.sleep(5)
            yield {"type": "audio", "data": b"never"}

    monkeypatch.setattr(engine.edge_tts, "Communicate", SlowCommunicate)
    with pytest.raises(TransientError, match=r"timed out|failed"):
        engine.synthesize_mp3("hello", "en-US-AriaNeural", timeout_s=0.05)


def test_timeout_env_override(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv(engine.TIMEOUT_ENV, raising=False)
    assert engine.timeout_from_env() == engine.DEFAULT_TIMEOUT_S
    monkeypatch.setenv(engine.TIMEOUT_ENV, "12")
    assert engine.timeout_from_env() == 12.0
    monkeypatch.setenv(engine.TIMEOUT_ENV, "oops")
    assert engine.timeout_from_env() == engine.DEFAULT_TIMEOUT_S
