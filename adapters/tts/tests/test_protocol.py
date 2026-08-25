"""The real CLI, spawned exactly the way morphod spawns it. No network."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest


def invoke(request: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    child_env = dict(os.environ)
    child_env.update(env or {})
    return subprocess.run(
        [sys.executable, "-m", "morpho_tts"],
        input=request,
        capture_output=True,
        text=True,
        timeout=60,
        env=child_env,
        check=False,
    )


@pytest.mark.parametrize("request_text", ["", "{oops", '{"op": 1}', "[1,2,3]", '{"params": {}}'])
def test_protocol_crash_exits_nonzero_with_empty_stdout(request_text: str) -> None:
    proc = invoke(request_text)
    assert proc.returncode != 0
    assert proc.stdout.strip() == ""


def test_unknown_op_is_a_clean_permanent_response() -> None:
    proc = invoke(json.dumps({"op": "tts.transcribe", "params": {}}))
    assert proc.returncode == 0
    payload = json.loads(proc.stdout)
    assert payload["ok"] is False
    assert payload["error"]["kind"] == "permanent"
    assert "tts.synthesize" in payload["error"]["message"]


def test_bad_params_are_permanent_and_exit_zero() -> None:
    proc = invoke(json.dumps({"op": "tts.synthesize", "params": {"voice": "en-US-AriaNeural"}}))
    assert proc.returncode == 0
    payload = json.loads(proc.stdout)
    assert payload["ok"] is False
    assert payload["error"]["kind"] == "permanent"
    assert payload["error"]["retry_after_ms"] == 0


def test_missing_ffmpeg_is_permanent_without_touching_the_network(tmp_path: Path) -> None:
    request = json.dumps(
        {
            "op": "tts.synthesize",
            "params": {
                "text": "benevolent",
                "voice": "en-US-AriaNeural",
                "format": "ogg_opus",
                "bitrate_kbps": 32,
                "out_path": str(tmp_path / "x.ogg"),
            },
        }
    )
    proc = invoke(request, env={"MORPHO_FFMPEG": "/nonexistent/ffmpeg"})
    assert proc.returncode == 0
    payload = json.loads(proc.stdout)
    assert payload["ok"] is False
    assert payload["error"]["kind"] == "permanent"
    assert "ffmpeg" in payload["error"]["message"]
    assert not (tmp_path / "x.ogg").exists()


def test_logs_go_to_stderr_only() -> None:
    proc = invoke(json.dumps({"op": "tts.nope", "params": {}}))
    assert proc.stdout.count("\n") == 1
    json.loads(proc.stdout)
    assert "tts" in proc.stderr


def test_log_level_env_silences_stderr() -> None:
    proc = invoke(
        json.dumps({"op": "tts.nope", "params": {}}),
        env={"MORPHO_ADAPTER_LOG_LEVEL": "CRITICAL"},
    )
    assert proc.returncode == 0
    assert proc.stderr.strip() == ""
    assert json.loads(proc.stdout)["ok"] is False
