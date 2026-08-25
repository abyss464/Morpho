"""Synthetic fixtures: no network, no ffmpeg, no edge-tts endpoint."""

from __future__ import annotations

import struct
from collections.abc import AsyncIterator, Iterable
from typing import Any, ClassVar

import pytest

OPUS_SAMPLE_RATE = 48_000
DEFAULT_PRE_SKIP = 312


def ogg_page(payload: bytes, granule: int, seq: int, header_type: int = 0) -> bytes:
    """One Ogg page. The CRC field is left zero — the parser does not verify it."""
    full, remainder = divmod(len(payload), 255)
    table = bytes([255] * full + [remainder])
    header = (
        b"OggS"
        + bytes([0, header_type])
        + struct.pack("<q", granule)
        + struct.pack("<I", 0xC0FFEE)
        + struct.pack("<I", seq)
        + struct.pack("<I", 0)
        + bytes([len(table)])
        + table
    )
    return header + payload


def opus_head(channels: int = 1, pre_skip: int = DEFAULT_PRE_SKIP) -> bytes:
    return (
        b"OpusHead"
        + bytes([1, channels])
        + struct.pack("<H", pre_skip)
        + struct.pack("<I", 48_000)
        + struct.pack("<h", 0)
        + bytes([0])
    )


def ogg_opus_bytes(
    duration_ms: int = 2140,
    *,
    channels: int = 1,
    pre_skip: int = DEFAULT_PRE_SKIP,
    audio_pages: int = 2,
) -> bytes:
    """A structurally valid Ogg Opus stream whose granule encodes `duration_ms`."""
    final_granule = round(duration_ms * OPUS_SAMPLE_RATE / 1000) + pre_skip
    pages = [
        ogg_page(opus_head(channels, pre_skip), granule=0, seq=0, header_type=0x02),
        ogg_page(b"OpusTags" + struct.pack("<I", 0) + struct.pack("<I", 0), granule=0, seq=1),
    ]
    for index in range(audio_pages):
        last = index == audio_pages - 1
        granule = final_granule if last else final_granule * (index + 1) // (audio_pages + 1)
        pages.append(
            ogg_page(b"\x78" * 96, granule=granule, seq=2 + index, header_type=0x04 if last else 0)
        )
    return b"".join(pages)


@pytest.fixture
def ogg_sample() -> bytes:
    return ogg_opus_bytes()


class FakeCommunicate:
    """Stands in for `edge_tts.Communicate`; records what it was asked for."""

    instances: ClassVar[list[FakeCommunicate]] = []

    def __init__(
        self,
        *,
        text: str,
        voice: str,
        rate: str = "+0%",
        pitch: str = "+0Hz",
        volume: str = "+0%",
    ) -> None:
        self.text = text
        self.voice = voice
        self.rate = rate
        self.pitch = pitch
        self.volume = volume
        type(self).instances.append(self)

    audio_chunks: Iterable[bytes] = (b"\xff\xfb\x90", b"payload-mp3")
    raises: BaseException | None = None

    async def stream(self) -> AsyncIterator[dict[str, Any]]:
        if type(self).raises is not None:
            raise type(self).raises
        yield {"type": "WordBoundary", "offset": 0, "duration": 1_000_000}
        for chunk in type(self).audio_chunks:
            yield {"type": "audio", "data": chunk}


@pytest.fixture
def fake_edge_tts(monkeypatch: pytest.MonkeyPatch) -> type[FakeCommunicate]:
    """Swap the real Communicate for the fake and reset its class-level state."""
    from morpho_tts import engine

    FakeCommunicate.instances = []
    FakeCommunicate.audio_chunks = (b"\xff\xfb\x90", b"payload-mp3")
    FakeCommunicate.raises = None
    monkeypatch.setattr(engine.edge_tts, "Communicate", FakeCommunicate)
    return FakeCommunicate


@pytest.fixture
def fake_ffmpeg(monkeypatch: pytest.MonkeyPatch, ogg_sample: bytes) -> list[list[str]]:
    """Replace ffmpeg discovery and execution; record every argv we would run."""
    from pathlib import Path

    from morpho_tts import transcode

    recorded: list[list[str]] = []

    def _find() -> str:
        return "/opt/pinned/ffmpeg"

    def _run(argv, *, timeout_s: float, what: str):  # type: ignore[no-untyped-def]
        recorded.append(list(argv))
        Path(argv[-1]).write_bytes(ogg_sample)
        return None

    monkeypatch.setattr(transcode, "find_ffmpeg", _find)
    monkeypatch.setattr(transcode, "run_binary", _run)
    return recorded
