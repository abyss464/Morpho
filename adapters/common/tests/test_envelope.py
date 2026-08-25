"""Envelope round-trips, exit-code discipline and stdout purity."""

from __future__ import annotations

import io
import json
from collections.abc import Mapping
from typing import Any

import pytest

from morpho_adapter_common import (
    EXIT_PROTOCOL_FAILURE,
    AdapterError,
    PermanentError,
    ProtocolError,
    RateLimitedError,
    TransientError,
    dispatch,
    parse_request,
    run,
)


def _run(request: str, ops: Mapping[str, Any]) -> tuple[int, dict[str, Any] | None, str]:
    stdin = io.StringIO(request)
    stdout = io.StringIO()
    code = run("test", ops, stdin=stdin, stdout=stdout)
    raw = stdout.getvalue()
    payload = json.loads(raw) if raw.strip() else None
    return code, payload, raw


def _echo(params: Mapping[str, Any]) -> dict[str, Any]:
    return {"echo": dict(params)}


# --- happy path -------------------------------------------------------------


def test_valid_request_round_trips() -> None:
    code, payload, raw = _run('{"op": "echo", "params": {"a": 1}}', {"echo": _echo})
    assert code == 0
    assert payload == {"ok": True, "result": {"echo": {"a": 1}}}
    assert raw.endswith("\n")
    assert raw.count("\n") == 1, "exactly one line of JSON on stdout"


def test_missing_params_defaults_to_empty_object() -> None:
    code, payload, _ = _run('{"op": "echo"}', {"echo": _echo})
    assert code == 0
    assert payload == {"ok": True, "result": {"echo": {}}}


def test_null_params_treated_as_empty() -> None:
    code, payload, _ = _run('{"op": "echo", "params": null}', {"echo": _echo})
    assert code == 0
    assert payload == {"ok": True, "result": {"echo": {}}}


def test_non_ascii_survives_the_round_trip() -> None:
    code, payload, _ = _run('{"op": "echo", "params": {"t": "café"}}', {"echo": _echo})
    assert code == 0
    assert payload is not None
    assert payload["result"]["echo"]["t"] == "café"


def test_response_bytes_are_stable_for_equal_results() -> None:
    request = '{"op": "echo", "params": {"b": 2, "a": 1}}'
    _, _, first = _run(request, {"echo": _echo})
    _, _, second = _run(request, {"echo": _echo})
    assert first == second


# --- protocol-level failures -> non-zero exit, nothing on stdout ------------


@pytest.mark.parametrize(
    "request_text",
    [
        "",
        "   \n ",
        "{not json}",
        '{"op": "echo"',
        '{"op": "echo", "params": {}} {"op": "echo"}',  # two documents
        "[]",
        '"a string"',
        "42",
        "null",
        '{"params": {}}',  # no op
        '{"op": "", "params": {}}',
        '{"op": 7}',
        '{"op": "echo", "params": []}',
        '{"op": "echo", "params": "nope"}',
    ],
)
def test_protocol_failures_exit_nonzero_with_silent_stdout(request_text: str) -> None:
    code, payload, raw = _run(request_text, {"echo": _echo})
    assert code == EXIT_PROTOCOL_FAILURE
    assert code != 0
    assert raw == ""
    assert payload is None


def test_parse_request_rejects_trailing_content() -> None:
    with pytest.raises(ProtocolError):
        parse_request('{"op": "a"} trailing')


# --- error taxonomy ---------------------------------------------------------


@pytest.mark.parametrize(
    ("exc", "kind", "retry_after_ms"),
    [
        (PermanentError("bad voice"), "permanent", 0),
        (TransientError("socket reset"), "transient", 0),
        (RateLimitedError("429", retry_after_ms=15_000), "rate_limited", 15_000),
        (RateLimitedError("429"), "rate_limited", 60_000),
    ],
)
def test_error_taxonomy_maps_onto_the_wire(
    exc: AdapterError, kind: str, retry_after_ms: int
) -> None:
    def boom(_: Mapping[str, Any]) -> dict[str, Any]:
        raise exc

    code, payload, _ = _run('{"op": "boom"}', {"boom": boom})
    assert code == 0, "an error response still honors the protocol"
    assert payload is not None
    assert payload["ok"] is False
    assert payload["error"]["kind"] == kind
    assert payload["error"]["message"] == str(exc)
    assert payload["error"]["retry_after_ms"] == retry_after_ms
    assert "result" not in payload


def test_unknown_op_is_permanent_not_a_crash() -> None:
    code, payload, _ = _run('{"op": "nope"}', {"echo": _echo})
    assert code == 0
    assert payload is not None
    assert payload["ok"] is False
    assert payload["error"]["kind"] == "permanent"
    assert "unsupported op" in payload["error"]["message"]
    assert "echo" in payload["error"]["message"]


def test_unexpected_exception_becomes_transient() -> None:
    def boom(_: Mapping[str, Any]) -> dict[str, Any]:
        raise ZeroDivisionError("division by zero")

    code, payload, _ = _run('{"op": "boom"}', {"boom": boom})
    assert code == 0
    assert payload is not None
    assert payload["error"]["kind"] == "transient"
    assert "ZeroDivisionError" in payload["error"]["message"]


def test_non_mapping_result_is_reported_not_emitted() -> None:
    def bad(_: Mapping[str, Any]) -> Any:
        return ["not", "a", "mapping"]

    code, payload, _ = _run('{"op": "bad"}', {"bad": bad})
    assert code == 0
    assert payload is not None
    assert payload["ok"] is False
    assert payload["error"]["kind"] == "transient"


def test_negative_retry_after_is_clamped() -> None:
    assert TransientError("x", retry_after_ms=-5).to_payload()["retry_after_ms"] == 0


# --- stdout purity ----------------------------------------------------------


def test_handler_prints_do_not_corrupt_stdout() -> None:
    def chatty(_: Mapping[str, Any]) -> dict[str, Any]:
        print("library noise on stdout")
        return {"ok": 1}

    code, payload, raw = _run('{"op": "chatty"}', {"chatty": chatty})
    assert code == 0
    assert payload == {"ok": True, "result": {"ok": 1}}
    assert "library noise" not in raw


def test_dispatch_is_usable_without_the_io_layer() -> None:
    assert dispatch({"echo": _echo}, "echo", {"x": 1}) == {"ok": True, "result": {"echo": {"x": 1}}}
