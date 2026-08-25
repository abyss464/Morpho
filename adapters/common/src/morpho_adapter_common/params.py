"""Typed accessors for request params.

Every failure here is a `PermanentError`: morphod sent something the adapter can
never fulfil, so retrying is pointless and the word should surface in dead
letters instead of looping.
"""

from __future__ import annotations

import os
from collections.abc import Mapping
from typing import Any

from .errors import PermanentError

_MISSING = object()


def _get(params: Mapping[str, Any], name: str, default: Any) -> Any:
    value = params.get(name, _MISSING)
    if value is _MISSING:
        if default is _MISSING:
            raise PermanentError(f"missing required param {name!r}")
        return default
    return value


def require_str(
    params: Mapping[str, Any],
    name: str,
    *,
    default: Any = _MISSING,
    allow_empty: bool = False,
) -> str:
    value = _get(params, name, default)
    if not isinstance(value, str):
        raise PermanentError(f"param {name!r} must be a string, got {_typename(value)}")
    if not allow_empty and not value.strip():
        raise PermanentError(f"param {name!r} must not be empty")
    return value


def require_int(
    params: Mapping[str, Any],
    name: str,
    *,
    default: Any = _MISSING,
    minimum: int | None = None,
    maximum: int | None = None,
) -> int:
    value = _get(params, name, default)
    if isinstance(value, bool) or not isinstance(value, int):
        raise PermanentError(f"param {name!r} must be an integer, got {_typename(value)}")
    if minimum is not None and value < minimum:
        raise PermanentError(f"param {name!r} must be >= {minimum}, got {value}")
    if maximum is not None and value > maximum:
        raise PermanentError(f"param {name!r} must be <= {maximum}, got {value}")
    return value


def require_str_list(
    params: Mapping[str, Any],
    name: str,
    *,
    allow_empty: bool = False,
) -> list[str]:
    value = _get(params, name, _MISSING)
    if not isinstance(value, list):
        raise PermanentError(f"param {name!r} must be an array, got {_typename(value)}")
    if not value and not allow_empty:
        raise PermanentError(f"param {name!r} must not be empty")
    out: list[str] = []
    for index, item in enumerate(value):
        if not isinstance(item, str):
            raise PermanentError(f"param {name}[{index}] must be a string, got {_typename(item)}")
        if not item.strip():
            raise PermanentError(f"param {name}[{index}] must not be empty")
        out.append(item)
    return out


def require_choice(
    params: Mapping[str, Any],
    name: str,
    allowed: tuple[str, ...],
    *,
    default: Any = _MISSING,
) -> str:
    value = require_str(params, name, default=default)
    if value not in allowed:
        raise PermanentError(f"param {name!r} must be one of {list(allowed)}, got {value!r}")
    return value


def require_out_path(params: Mapping[str, Any], name: str = "out_path") -> str:
    """Validate an engine-owned output path and make sure its directory exists.

    morphod owns the temp path; adapters never touch `data/` themselves.
    """
    value = require_str(params, name)
    if not os.path.isabs(value):
        raise PermanentError(f"param {name!r} must be an absolute path, got {value!r}")
    parent = os.path.dirname(value) or "/"
    if not os.path.isdir(parent):
        raise PermanentError(f"output directory does not exist: {parent}")
    return value


def _typename(value: Any) -> str:
    return "null" if value is None else type(value).__name__
