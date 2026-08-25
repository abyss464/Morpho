"""Exact duration of an Ogg Opus stream, straight from the container.

Opus always decodes at 48 kHz, and the granule position on the final Ogg page
counts 48 kHz samples including the encoder's pre-skip. Reading it beats
shelling out to ffprobe: no extra process, no parsing of localised output, and
the answer is exact rather than a rounded container estimate.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Final

OPUS_SAMPLE_RATE: Final[int] = 48_000
_CAPTURE: Final[bytes] = b"OggS"
_HEADER_LEN: Final[int] = 27


class OggParseError(ValueError):
    """The byte stream is not a well-formed Ogg Opus file."""


@dataclass(frozen=True)
class OpusInfo:
    channels: int
    pre_skip: int
    final_granule: int

    @property
    def sample_count(self) -> int:
        return max(0, self.final_granule - self.pre_skip)

    @property
    def duration_ms(self) -> int:
        return round(self.sample_count * 1000 / OPUS_SAMPLE_RATE)


def parse(data: bytes) -> OpusInfo:
    """Walk every Ogg page; return the OpusHead fields and the last granule."""
    if len(data) < _HEADER_LEN or not data.startswith(_CAPTURE):
        raise OggParseError("not an Ogg stream (missing 'OggS' capture pattern)")

    offset = 0
    total = len(data)
    channels: int | None = None
    pre_skip: int | None = None
    final_granule = 0
    pages = 0

    while offset + _HEADER_LEN <= total:
        if data[offset : offset + 4] != _CAPTURE:
            raise OggParseError(f"lost page alignment at byte {offset}")
        version = data[offset + 4]
        if version != 0:
            raise OggParseError(f"unsupported Ogg page version {version}")
        granule = struct.unpack_from("<q", data, offset + 6)[0]
        segment_count = data[offset + 26]
        table_end = offset + _HEADER_LEN + segment_count
        if table_end > total:
            raise OggParseError("truncated Ogg segment table")
        payload_len = sum(data[offset + _HEADER_LEN : table_end])
        payload_end = table_end + payload_len
        if payload_end > total:
            raise OggParseError("truncated Ogg page payload")

        if channels is None:
            payload = data[table_end:payload_end]
            if not payload.startswith(b"OpusHead") or len(payload) < 12:
                raise OggParseError("first Ogg page is not an OpusHead (not an Opus stream)")
            channels = payload[9]
            pre_skip = struct.unpack_from("<H", payload, 10)[0]

        if granule >= 0:
            final_granule = granule
        offset = payload_end
        pages += 1

    if channels is None or pre_skip is None:
        raise OggParseError("no OpusHead found")
    if offset != total:
        raise OggParseError("trailing bytes after the last Ogg page")
    if pages < 2:
        raise OggParseError("Opus stream has no audio pages")

    return OpusInfo(channels=channels, pre_skip=pre_skip, final_granule=final_granule)


def parse_file(path: str | Path) -> OpusInfo:
    return parse(Path(path).read_bytes())
