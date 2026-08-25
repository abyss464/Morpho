"""The real CLI, spawned the way morphod spawns it, with no backend present."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

from conftest import closed_loopback_port


def invoke(request: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    child_env = dict(os.environ)
    child_env.update(env or {})
    return subprocess.run(
        [sys.executable, "-m", "morpho_sdxl"],
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
    proc = invoke(json.dumps({"op": "sdxl.upscale", "params": {}}))
    assert proc.returncode == 0
    payload = json.loads(proc.stdout)
    assert payload["ok"] is False
    assert payload["error"]["kind"] == "permanent"
    assert "sdxl.generate" in payload["error"]["message"]


def test_an_absent_backend_reports_the_contract_message(tmp_path: Path) -> None:
    port = closed_loopback_port()
    request = json.dumps(
        {
            "op": "sdxl.generate",
            "params": {
                "prompt": "a scene depicting the concept of 'abandon'",
                "negative_prompt": "text, watermark, logo",
                "seed": 42,
                "width": 768,
                "height": 576,
                "out_path": str(tmp_path / "x.webp"),
            },
        }
    )
    proc = invoke(
        request,
        env={
            "COMFYUI_URL": f"http://127.0.0.1:{port}",
            "MORPHO_SDXL_CONNECT_TIMEOUT_S": "2",
        },
    )
    assert proc.returncode == 0, "an unconfigured backend is a response, not a crash"
    payload = json.loads(proc.stdout)
    assert payload["ok"] is False
    assert payload["error"]["kind"] == "permanent"
    assert payload["error"]["message"].startswith("sdxl backend not configured")
    assert payload["error"]["retry_after_ms"] == 0
    assert not (tmp_path / "x.webp").exists()


def test_bad_params_never_touch_the_backend(tmp_path: Path) -> None:
    proc = invoke(
        json.dumps({"op": "sdxl.generate", "params": {"prompt": "", "out_path": str(tmp_path)}}),
        env={"COMFYUI_URL": f"http://127.0.0.1:{closed_loopback_port()}"},
    )
    assert proc.returncode == 0
    payload = json.loads(proc.stdout)
    assert payload["error"]["kind"] == "permanent"
    assert "sdxl backend" not in payload["error"]["message"]


def test_stdout_carries_one_json_line_and_logs_go_to_stderr() -> None:
    proc = invoke(json.dumps({"op": "sdxl.nope", "params": {}}))
    assert proc.stdout.count("\n") == 1
    json.loads(proc.stdout)
    assert "sdxl" in proc.stderr
