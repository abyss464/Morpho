"""Duration is read from the Ogg granule position, not guessed."""

from __future__ import annotations

import pytest

from conftest import DEFAULT_PRE_SKIP, ogg_opus_bytes, ogg_page, opus_head
from morpho_tts.oggopus import OggParseError, parse, parse_file


@pytest.mark.parametrize("duration_ms", [1, 250, 2140, 9_999, 60_000])
def test_duration_round_trips(duration_ms: int) -> None:
    info = parse(ogg_opus_bytes(duration_ms))
    assert info.duration_ms == duration_ms


def test_pre_skip_is_subtracted() -> None:
    info = parse(ogg_opus_bytes(2000, pre_skip=DEFAULT_PRE_SKIP))
    assert info.pre_skip == DEFAULT_PRE_SKIP
    assert info.final_granule == round(2000 * 48_000 / 1000) + DEFAULT_PRE_SKIP
    assert info.sample_count == 96_000


def test_channels_are_reported() -> None:
    assert parse(ogg_opus_bytes(500, channels=1)).channels == 1
    assert parse(ogg_opus_bytes(500, channels=2)).channels == 2


def test_multi_page_streams_use_the_last_granule() -> None:
    assert parse(ogg_opus_bytes(3000, audio_pages=5)).duration_ms == 3000


def test_large_payload_spanning_many_segments() -> None:
    pages = [
        ogg_page(opus_head(), granule=0, seq=0, header_type=0x02),
        ogg_page(b"OpusTags" + b"\x00" * 8, granule=0, seq=1),
        ogg_page(b"\x41" * 1200, granule=48_312, seq=2, header_type=0x04),
    ]
    info = parse(b"".join(pages))
    assert info.duration_ms == 1000


def test_parse_file(tmp_path) -> None:
    path = tmp_path / "a.ogg"
    path.write_bytes(ogg_opus_bytes(1234))
    assert parse_file(path).duration_ms == 1234


@pytest.mark.parametrize(
    "data",
    [
        b"",
        b"NotOggAtAll",
        b"OggS" + b"\x00" * 10,
        ogg_opus_bytes(1000)[:-40],
        ogg_opus_bytes(1000) + b"garbage",
    ],
)
def test_malformed_streams_raise(data: bytes) -> None:
    with pytest.raises(OggParseError):
        parse(data)


def test_stream_without_opushead_raises() -> None:
    page = ogg_page(b"NotOpus" + b"\x00" * 12, granule=0, seq=0, header_type=0x02)
    with pytest.raises(OggParseError, match="OpusHead"):
        parse(page)


def test_header_only_stream_has_no_audio() -> None:
    page = ogg_page(opus_head(), granule=0, seq=0, header_type=0x02)
    with pytest.raises(OggParseError, match="no audio pages"):
        parse(page)
