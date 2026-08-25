"""`tts.synthesize` end to end with edge-tts and ffmpeg both faked out."""

from __future__ import annotations

from pathlib import Path

import pytest
from morpho_adapter_common import PermanentError, dispatch

from conftest import ogg_opus_bytes
from morpho_tts import ops, transcode
from morpho_tts.ops import OPS, synthesize


def _params(tmp_path: Path, **overrides: object) -> dict[str, object]:
    params: dict[str, object] = {
        "text": "well meaning and kindly",
        "voice": "en-US-AriaNeural",
        "rate": "+0%",
        "pitch": "+0Hz",
        "format": "ogg_opus",
        "bitrate_kbps": 32,
        "out_path": str(tmp_path / "x.ogg"),
    }
    params.update(overrides)
    return params


def test_contract_example_shape(tmp_path: Path, fake_edge_tts, fake_ffmpeg) -> None:
    result = synthesize(_params(tmp_path))
    assert set(result) == {"duration_ms", "engine_ver"}
    assert result["duration_ms"] == 2140
    assert result["engine_ver"].startswith("edge-tts/")
    assert (tmp_path / "x.ogg").read_bytes() == ogg_opus_bytes()


def test_response_envelope(tmp_path: Path, fake_edge_tts, fake_ffmpeg) -> None:
    response = dispatch(OPS, "tts.synthesize", _params(tmp_path))
    assert response["ok"] is True
    assert response["result"]["duration_ms"] == 2140


def test_requested_bitrate_reaches_ffmpeg(tmp_path: Path, fake_edge_tts, fake_ffmpeg) -> None:
    synthesize(_params(tmp_path, bitrate_kbps=48))
    assert "48k" in fake_ffmpeg[-1]
    assert "-ac" in fake_ffmpeg[-1]


def test_text_and_voice_reach_edge_tts(tmp_path: Path, fake_edge_tts, fake_ffmpeg) -> None:
    synthesize(_params(tmp_path, text="benevolent", voice="en-GB-SoniaNeural"))
    call = fake_edge_tts.instances[-1]
    assert call.text == "benevolent"
    assert call.voice == "en-GB-SoniaNeural"


def test_defaults_when_optional_params_are_absent(
    tmp_path: Path, fake_edge_tts, fake_ffmpeg
) -> None:
    result = synthesize(
        {"text": "hello", "voice": "en-US-AriaNeural", "out_path": str(tmp_path / "y.ogg")}
    )
    assert result["duration_ms"] == 2140
    assert f"{ops.DEFAULT_BITRATE_KBPS}k" in fake_ffmpeg[-1]
    call = fake_edge_tts.instances[-1]
    assert (call.rate, call.pitch) == ("+0%", "+0Hz")


def test_no_partial_file_when_ffmpeg_fails(
    tmp_path: Path, fake_edge_tts, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(transcode, "find_ffmpeg", lambda: "/opt/pinned/ffmpeg")

    def _half_write(argv, *, timeout_s, what):  # type: ignore[no-untyped-def]
        Path(argv[-1]).write_bytes(b"truncated")
        raise PermanentError("ffmpeg exited 1: boom")

    monkeypatch.setattr(transcode, "run_binary", _half_write)
    out = tmp_path / "z.ogg"
    with pytest.raises(PermanentError):
        synthesize(_params(tmp_path, out_path=str(out)))
    assert not out.exists()
    assert list(tmp_path.iterdir()) == []


def test_unreadable_output_is_permanent(
    tmp_path: Path, fake_edge_tts, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(transcode, "find_ffmpeg", lambda: "/opt/pinned/ffmpeg")

    def _garbage(argv, *, timeout_s, what):  # type: ignore[no-untyped-def]
        Path(argv[-1]).write_bytes(b"not an ogg file at all")

    monkeypatch.setattr(transcode, "run_binary", _garbage)
    with pytest.raises(PermanentError, match="unreadable Ogg Opus"):
        synthesize(_params(tmp_path))


def test_missing_ffmpeg_short_circuits_before_synthesis(
    tmp_path: Path, fake_edge_tts, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv(transcode.FFMPEG_ENV, "/nonexistent/ffmpeg")
    with pytest.raises(PermanentError) as excinfo:
        synthesize(_params(tmp_path))
    assert excinfo.value.kind == "permanent"
    assert fake_edge_tts.instances == [], "no network call once ffmpeg is known to be missing"


@pytest.mark.parametrize(
    "overrides",
    [
        {"text": ""},
        {"text": 42},
        {"voice": ""},
        {"format": "mp3"},
        {"bitrate_kbps": 0},
        {"bitrate_kbps": 1000},
        {"bitrate_kbps": "32"},
        {"rate": "fast"},
        {"pitch": "high"},
        {"out_path": "relative.ogg"},
        {"text": "x" * (ops.MAX_TEXT_CHARS + 1)},
    ],
)
def test_bad_params_are_permanent(
    tmp_path: Path, fake_edge_tts, fake_ffmpeg, overrides: dict[str, object]
) -> None:
    with pytest.raises(PermanentError):
        synthesize(_params(tmp_path, **overrides))


def test_missing_required_params_are_permanent(tmp_path: Path, fake_edge_tts, fake_ffmpeg) -> None:
    for params in ({}, {"text": "a"}, {"voice": "v"}, {"text": "a", "voice": "v"}):
        with pytest.raises(PermanentError):
            synthesize(params)


def test_out_path_directory_must_exist(tmp_path: Path, fake_edge_tts, fake_ffmpeg) -> None:
    with pytest.raises(PermanentError, match="output directory"):
        synthesize(_params(tmp_path, out_path=str(tmp_path / "nope" / "x.ogg")))
