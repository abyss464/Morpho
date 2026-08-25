"""Shared protocol layer for the Morpho Python adapters."""

from __future__ import annotations

from .envelope import (
    EXIT_PROTOCOL_FAILURE,
    Handler,
    configure_logging,
    dispatch,
    failure,
    parse_request,
    run,
    success,
    write_response,
)
from .errors import (
    DEFAULT_RATE_LIMIT_MS,
    ERROR_KINDS,
    PERMANENT,
    RATE_LIMITED,
    TRANSIENT,
    AdapterError,
    ErrorKind,
    PermanentError,
    ProtocolError,
    RateLimitedError,
    TransientError,
)
from .outfile import staged_output, write_bytes_atomic
from .params import (
    require_choice,
    require_int,
    require_out_path,
    require_str,
    require_str_list,
)
from .proc import resolve_binary, run_binary

__all__ = [
    "DEFAULT_RATE_LIMIT_MS",
    "ERROR_KINDS",
    "EXIT_PROTOCOL_FAILURE",
    "PERMANENT",
    "RATE_LIMITED",
    "TRANSIENT",
    "AdapterError",
    "ErrorKind",
    "Handler",
    "PermanentError",
    "ProtocolError",
    "RateLimitedError",
    "TransientError",
    "configure_logging",
    "dispatch",
    "failure",
    "parse_request",
    "require_choice",
    "require_int",
    "require_out_path",
    "require_str",
    "require_str_list",
    "resolve_binary",
    "run",
    "run_binary",
    "staged_output",
    "success",
    "write_bytes_atomic",
    "write_response",
]
