"""Atomic output staging and external-binary discovery."""

from __future__ import annotations

import sys

import pytest

from morpho_adapter_common import (
    PermanentError,
    TransientError,
    resolve_binary,
    run_binary,
    staged_output,
    write_bytes_atomic,
)


def test_staged_output_renames_on_success(tmp_path) -> None:
    target = tmp_path / "out.bin"
    with staged_output(target) as staging:
        assert staging != target
        staging.write_bytes(b"payload")
        assert not target.exists(), "nothing visible at out_path until the block finishes"
    assert target.read_bytes() == b"payload"
    assert not staging.exists()


def test_staged_output_leaves_no_partial_file_on_failure(tmp_path) -> None:
    target = tmp_path / "out.bin"
    with pytest.raises(RuntimeError), staged_output(target) as staging:
        staging.write_bytes(b"half")
        raise RuntimeError("boom")
    assert not target.exists()
    assert list(tmp_path.iterdir()) == []


def test_staged_output_rejects_an_empty_block(tmp_path) -> None:
    target = tmp_path / "out.bin"
    with pytest.raises(FileNotFoundError), staged_output(target):
        pass
    assert not target.exists()


def test_write_bytes_atomic(tmp_path) -> None:
    target = tmp_path / "out.bin"
    write_bytes_atomic(target, b"abc")
    assert target.read_bytes() == b"abc"


def test_resolve_binary_missing_is_permanent() -> None:
    with pytest.raises(PermanentError) as excinfo:
        resolve_binary("definitely-not-a-real-binary-xyz")
    assert excinfo.value.kind == "permanent"


def test_resolve_binary_honors_env_override(monkeypatch) -> None:
    monkeypatch.setenv("MORPHO_TEST_BIN", sys.executable)
    assert resolve_binary("python-nope", env_var="MORPHO_TEST_BIN") == sys.executable
    monkeypatch.setenv("MORPHO_TEST_BIN", "/nonexistent/path/to/bin")
    with pytest.raises(PermanentError):
        resolve_binary("python-nope", env_var="MORPHO_TEST_BIN")


def test_run_binary_success() -> None:
    proc = run_binary([sys.executable, "-c", "print('hi')"], timeout_s=30, what="python")
    assert proc.stdout.strip() == b"hi"


def test_run_binary_nonzero_exit_is_permanent() -> None:
    with pytest.raises(PermanentError) as excinfo:
        run_binary(
            [sys.executable, "-c", "import sys; sys.stderr.write('bad args\\n'); sys.exit(3)"],
            timeout_s=30,
            what="python",
        )
    assert "exited 3" in excinfo.value.message
    assert "bad args" in excinfo.value.message


def test_run_binary_timeout_is_transient() -> None:
    with pytest.raises(TransientError):
        run_binary(
            [sys.executable, "-c", "import time; time.sleep(5)"],
            timeout_s=0.3,
            what="python",
        )
