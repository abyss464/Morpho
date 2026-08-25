"""The real CLI, spawned exactly the way morphod spawns it. No network."""

from __future__ import annotations

import json
import os
import subprocess
import sys

import pytest


def invoke(request: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    child_env = dict(os.environ)
    child_env.pop("MORPHO_MORFESSOR_MODEL", None)
    child_env.update(env or {})
    return subprocess.run(
        [sys.executable, "-m", "morpho_morfessor"],
        input=request,
        capture_output=True,
        text=True,
        timeout=120,
        env=child_env,
        check=False,
    )


def test_contract_request_round_trips() -> None:
    proc = invoke(
        json.dumps({"op": "morfessor.segment", "params": {"words": ["benevolent", "unhappiness"]}})
    )
    assert proc.returncode == 0
    payload = json.loads(proc.stdout)
    assert payload["ok"] is True
    result = payload["result"]
    assert set(result["segments"]) == {"benevolent", "unhappiness"}
    assert result["model_ver"].startswith("morfessor/")


def test_stdout_carries_one_json_line_and_nothing_else() -> None:
    proc = invoke(json.dumps({"op": "morfessor.segment", "params": {"words": ["benevolent"]}}))
    assert proc.stdout.count("\n") == 1
    json.loads(proc.stdout)


@pytest.mark.parametrize("request_text", ["", "{oops", '{"op": 1}', "[1,2,3]", '{"params": {}}'])
def test_protocol_crash_exits_nonzero_with_empty_stdout(request_text: str) -> None:
    proc = invoke(request_text)
    assert proc.returncode != 0
    assert proc.stdout.strip() == ""


def test_unknown_op_is_a_clean_permanent_response() -> None:
    proc = invoke(json.dumps({"op": "morfessor.train", "params": {}}))
    assert proc.returncode == 0
    payload = json.loads(proc.stdout)
    assert payload["ok"] is False
    assert payload["error"]["kind"] == "permanent"
    assert "morfessor.segment" in payload["error"]["message"]


def test_bad_params_are_permanent_and_exit_zero() -> None:
    proc = invoke(json.dumps({"op": "morfessor.segment", "params": {"words": []}}))
    assert proc.returncode == 0
    payload = json.loads(proc.stdout)
    assert payload["ok"] is False
    assert payload["error"]["kind"] == "permanent"


def test_a_missing_model_override_is_permanent() -> None:
    proc = invoke(
        json.dumps({"op": "morfessor.segment", "params": {"words": ["benevolent"]}}),
        env={"MORPHO_MORFESSOR_MODEL": "/nonexistent/model.bin"},
    )
    assert proc.returncode == 0
    payload = json.loads(proc.stdout)
    assert payload["ok"] is False
    assert payload["error"]["kind"] == "permanent"


def test_two_processes_agree(tmp_path) -> None:
    words = [f"un{root}ness" for root in ("happi", "nation", "cover", "form", "port")]
    request = json.dumps({"op": "morfessor.segment", "params": {"words": words}})
    first = json.loads(invoke(request).stdout)
    second = json.loads(invoke(request).stdout)
    assert first == second, "same request, same bytes — across process boundaries"
