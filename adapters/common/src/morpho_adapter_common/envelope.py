"""The stdin/stdout JSON envelope shared by every Morpho adapter.

Contract (`docs/contracts/adapter-protocol.md`):

    request  {"op": "<name>", "params": {...}}
    response {"ok": true,  "result": {...}}
             {"ok": false, "error": {"kind": ..., "message": ..., "retry_after_ms": 0}}

stdout carries the response JSON and nothing else; every log line goes to
stderr. Exit code 0 whenever the protocol was honored — including error
responses — and non-zero only when the request could not be parsed at all.
"""

from __future__ import annotations

import contextlib
import io
import json
import logging
import os
import sys
import time
import traceback
from collections.abc import Callable, Mapping
from typing import Any, Final, TextIO

from .errors import AdapterError, PermanentError, ProtocolError, TransientError

#: Handler signature: params mapping -> JSON-serialisable result mapping.
Handler = Callable[[Mapping[str, Any]], Mapping[str, Any]]

#: Exit code for a protocol-level failure (unreadable stdin / malformed JSON /
#: envelope that is not an object with a string ``op``). morphod treats any
#: non-zero exit as a crashed adapter.
EXIT_PROTOCOL_FAILURE: Final[int] = 2

_LOG_ENV: Final[str] = "MORPHO_ADAPTER_LOG_LEVEL"

log = logging.getLogger("morpho.adapter")


def configure_logging(adapter: str) -> None:
    """Send logs to stderr only. stdout is reserved for the response JSON."""
    level_name = os.environ.get(_LOG_ENV, "INFO").upper()
    level = getattr(logging, level_name, logging.INFO)
    if not isinstance(level, int):
        level = logging.INFO
    root = logging.getLogger()
    for handler in list(root.handlers):
        root.removeHandler(handler)
    handler = logging.StreamHandler(sys.stderr)
    handler.setFormatter(
        logging.Formatter(
            fmt=f"%(asctime)s %(levelname)s {adapter}: %(message)s",
            datefmt="%Y-%m-%dT%H:%M:%S",
        )
    )
    root.addHandler(handler)
    root.setLevel(level)


def success(result: Mapping[str, Any]) -> dict[str, Any]:
    return {"ok": True, "result": dict(result)}


def failure(error: AdapterError) -> dict[str, Any]:
    return {"ok": False, "error": error.to_payload()}


def parse_request(raw: str) -> tuple[str, Mapping[str, Any]]:
    """Parse one request envelope, or raise `ProtocolError`."""
    if not raw.strip():
        raise ProtocolError("empty request on stdin")
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ProtocolError(f"malformed JSON request: {exc}") from exc
    if not isinstance(payload, dict):
        raise ProtocolError("request must be a JSON object")
    op = payload.get("op")
    if not isinstance(op, str) or not op:
        raise ProtocolError("request field 'op' must be a non-empty string")
    params = payload.get("params", {})
    if params is None:
        params = {}
    if not isinstance(params, dict):
        raise ProtocolError("request field 'params' must be a JSON object")
    return op, params


def dispatch(ops: Mapping[str, Handler], op: str, params: Mapping[str, Any]) -> dict[str, Any]:
    """Run one op and translate any outcome into a response envelope.

    An unknown op is a `permanent` error rather than a crash: the envelope was
    honored, and retrying a request this binary cannot serve is pointless.
    Unexpected exceptions become `transient` — the adapter cannot tell a flaky
    dependency from its own bug, and morphod's bounded retries end in a dead
    letter either way. The traceback goes to stderr for `last_error`.
    """
    handler = ops.get(op)
    if handler is None:
        known = ", ".join(sorted(ops))
        log.error("unsupported op %r; this adapter serves: %s", op, known)
        return failure(PermanentError(f"unsupported op {op!r}; this adapter serves: {known}"))
    started = time.monotonic()
    try:
        result = handler(params)
    except AdapterError as exc:
        elapsed_ms = int((time.monotonic() - started) * 1000)
        log.warning("op %s failed as %s after %d ms: %s", op, exc.kind, elapsed_ms, exc.message)
        return failure(exc)
    except Exception as exc:
        elapsed_ms = int((time.monotonic() - started) * 1000)
        log.error("op %s raised %s after %d ms", op, type(exc).__name__, elapsed_ms)
        log.error("%s", traceback.format_exc())
        return failure(TransientError(f"unhandled {type(exc).__name__}: {exc}"))
    if not isinstance(result, Mapping):
        log.error("op %s returned %s, expected a mapping", op, type(result).__name__)
        return failure(TransientError(f"op {op} produced a non-object result"))
    elapsed_ms = int((time.monotonic() - started) * 1000)
    log.info("op %s ok in %d ms", op, elapsed_ms)
    return success(result)


def write_response(response: Mapping[str, Any], stream: TextIO) -> None:
    """Emit exactly one line of JSON, keys sorted so output bytes are stable."""
    stream.write(json.dumps(response, ensure_ascii=False, sort_keys=True, separators=(",", ":")))
    stream.write("\n")
    stream.flush()


def run(
    adapter: str,
    ops: Mapping[str, Handler],
    *,
    stdin: TextIO | None = None,
    stdout: TextIO | None = None,
) -> int:
    """Full adapter lifecycle. Returns the process exit code."""
    configure_logging(adapter)
    source = stdin if stdin is not None else sys.stdin
    sink = stdout if stdout is not None else sys.stdout

    try:
        raw = source.read()
    except OSError as exc:
        log.error("cannot read stdin: %s", exc)
        return EXIT_PROTOCOL_FAILURE

    try:
        op, params = parse_request(raw)
    except ProtocolError as exc:
        log.error("protocol failure: %s", exc)
        return EXIT_PROTOCOL_FAILURE

    log.info("op %s starting", op)
    # Third-party libraries occasionally print to stdout; that would corrupt the
    # response. Everything a handler writes to stdout is folded into stderr.
    guard = io.StringIO()
    with contextlib.redirect_stdout(guard):
        response = dispatch(ops, op, params)
    stray = guard.getvalue()
    if stray:
        log.warning("handler wrote %d bytes to stdout; redirected: %s", len(stray), stray.strip())

    write_response(response, sink)
    return 0
