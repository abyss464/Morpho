"""ffmpeg invocation is pinned, and a missing transcoder is permanent."""

from __future__ import annotations

from pathlib import Path

import pytest
from morpho_adapter_common import PermanentError, TransientError

from morpho_tts import transcode


def test_argv_pins_every_byte_affecting_flag() -> None:
    argv = transcode.build_argv("/usr/bin/ffmpeg", Path("/in.mp3"), Path("/out.ogg"), 32)
    joined = " ".join(argv)
    assert argv[0] == "/usr/bin/ffmpeg"
    assert argv[-1] == "/out.ogg"
    assert "-i /in.mp3" in joined
    assert "-c:a libopus" in joined
    assert "-b:a 32k" in joined
    assert "-ac 1" in joined, "mono per the release budget"
    assert "-ar 48000" in joined, "Opus decodes at 48 kHz regardless of the source"
    assert "-map_metadata -1" in joined
    assert "-f ogg" in joined
    assert argv.count("+bitexact") == 3, "bitexact on the demuxer, the codec and the muxer"
    assert "-nostdin" in argv, "never block waiting on stdin"


def test_bitrate_reaches_the_command_line() -> None:
    for kbps in (16, 32, 48):
        argv = transcode.build_argv("ffmpeg", Path("a.mp3"), Path("b.ogg"), kbps)
        assert f"{kbps}k" in argv


def test_missing_ffmpeg_is_permanent(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(transcode.FFMPEG_ENV, "/nonexistent/bin/ffmpeg")
    with pytest.raises(PermanentError) as excinfo:
        transcode.find_ffmpeg()
    assert excinfo.value.kind == "permanent"
    assert "ffmpeg" in excinfo.value.message


def test_missing_ffmpeg_on_path_is_permanent(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv(transcode.FFMPEG_ENV, raising=False)
    monkeypatch.setattr("shutil.which", lambda _name: None)
    with pytest.raises(PermanentError) as excinfo:
        transcode.find_ffmpeg()
    assert "not found on PATH" in excinfo.value.message


def test_build_without_libopus_gets_an_actionable_message(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    def _boom(argv, *, timeout_s, what):  # type: ignore[no-untyped-def]
        raise PermanentError("ffmpeg exited 1: Unknown encoder 'libopus'")

    monkeypatch.setattr(transcode, "run_binary", _boom)
    with pytest.raises(PermanentError) as excinfo:
        transcode.to_ogg_opus(tmp_path / "a.mp3", tmp_path / "b.ogg", 32, ffmpeg="ffmpeg")
    assert "--enable-libopus" in excinfo.value.message


def test_other_ffmpeg_failures_pass_through(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    def _boom(argv, *, timeout_s, what):  # type: ignore[no-untyped-def]
        raise PermanentError("ffmpeg exited 1: Invalid data found when processing input")

    monkeypatch.setattr(transcode, "run_binary", _boom)
    with pytest.raises(PermanentError) as excinfo:
        transcode.to_ogg_opus(tmp_path / "a.mp3", tmp_path / "b.ogg", 32, ffmpeg="ffmpeg")
    assert "Invalid data" in excinfo.value.message
    assert "--enable-libopus" not in excinfo.value.message


def test_ffmpeg_timeout_stays_transient(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    def _boom(argv, *, timeout_s, what):  # type: ignore[no-untyped-def]
        raise TransientError("ffmpeg timed out after 30s")

    monkeypatch.setattr(transcode, "run_binary", _boom)
    with pytest.raises(TransientError):
        transcode.to_ogg_opus(tmp_path / "a.mp3", tmp_path / "b.ogg", 32, ffmpeg="ffmpeg")


def test_silent_success_with_no_output_is_permanent(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setattr(transcode, "run_binary", lambda *a, **k: None)
    with pytest.raises(PermanentError, match="no audio"):
        transcode.to_ogg_opus(tmp_path / "a.mp3", tmp_path / "b.ogg", 32, ffmpeg="ffmpeg")


def test_timeout_env_override(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv(transcode.FFMPEG_TIMEOUT_ENV, raising=False)
    assert transcode._timeout_s() == transcode.DEFAULT_FFMPEG_TIMEOUT_S
    monkeypatch.setenv(transcode.FFMPEG_TIMEOUT_ENV, "7.5")
    assert transcode._timeout_s() == 7.5
    monkeypatch.setenv(transcode.FFMPEG_TIMEOUT_ENV, "nonsense")
    assert transcode._timeout_s() == transcode.DEFAULT_FFMPEG_TIMEOUT_S
    monkeypatch.setenv(transcode.FFMPEG_TIMEOUT_ENV, "-3")
    assert transcode._timeout_s() == transcode.DEFAULT_FFMPEG_TIMEOUT_S
