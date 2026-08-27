"""The envelope: what morphod actually sends and reads."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

from morpho_codex.ops import OP_GENERATE, OPS

ADAPTER_ROOT = Path(__file__).resolve().parents[1]


def call(request: dict) -> tuple[int, dict]:
    proc = subprocess.run(
        [sys.executable, "-m", "morpho_codex"],
        input=json.dumps(request).encode(),
        capture_output=True,
        cwd=ADAPTER_ROOT,
        env={
            "PATH": "/usr/bin:/bin",
            "PYTHONPATH": str(ADAPTER_ROOT / "src"),
        },
        check=False,
        timeout=60,
    )
    return proc.returncode, json.loads(proc.stdout)


def test_the_op_is_registered_under_its_contract_name():
    assert OP_GENERATE == "codex.generate"
    assert set(OPS) == {OP_GENERATE}


def test_an_unknown_op_is_a_permanent_failure_with_exit_zero():
    """Ruling #3: the envelope was honoured, so the exit code is 0 and the
    failure is in the payload."""
    code, body = call({"op": "codex.embed", "params": {}})
    assert code == 0
    assert body["ok"] is False
    assert body["error"]["kind"] == "permanent"


def test_a_missing_generator_reports_the_contract_message(tmp_path):
    """`PATH` here holds no `codex`, which is the state of every machine that
    has not installed one — and the answer must be an actionable permanent
    failure rather than a crash."""
    code, body = call(
        {
            "op": "codex.generate",
            "params": {
                "word_id": 7,
                "lemma": "abandon",
                "slot1_sentence": "She had to abandon the car in the flood.",
                "prompt_ver": "codex/1",
                "width": 768,
                "height": 576,
                "out_path": str(tmp_path / "image.webp"),
            },
        }
    )
    assert code == 0
    assert body["ok"] is False
    assert body["error"]["kind"] == "permanent"
    assert "codex backend not configured" in body["error"]["message"]
