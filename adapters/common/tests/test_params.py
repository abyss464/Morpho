"""Param validation always fails as `permanent` — morphod must not retry it."""

from __future__ import annotations

import pytest

from morpho_adapter_common import (
    PermanentError,
    require_choice,
    require_int,
    require_out_path,
    require_str,
    require_str_list,
)


def test_require_str_accepts_and_defaults() -> None:
    assert require_str({"a": "x"}, "a") == "x"
    assert require_str({}, "a", default="fallback") == "fallback"
    assert require_str({"a": ""}, "a", allow_empty=True) == ""


@pytest.mark.parametrize("params", [{}, {"a": None}, {"a": 3}, {"a": "   "}])
def test_require_str_rejects(params: dict[str, object]) -> None:
    with pytest.raises(PermanentError):
        require_str(params, "a")


def test_require_int_bounds() -> None:
    assert require_int({"n": 32}, "n", minimum=8, maximum=64) == 32
    with pytest.raises(PermanentError):
        require_int({"n": 4}, "n", minimum=8)
    with pytest.raises(PermanentError):
        require_int({"n": 512}, "n", maximum=64)
    with pytest.raises(PermanentError):
        require_int({"n": True}, "n")
    with pytest.raises(PermanentError):
        require_int({"n": 3.5}, "n")


def test_require_str_list() -> None:
    assert require_str_list({"w": ["a", "b"]}, "w") == ["a", "b"]
    assert require_str_list({"w": []}, "w", allow_empty=True) == []
    with pytest.raises(PermanentError):
        require_str_list({"w": []}, "w")
    with pytest.raises(PermanentError):
        require_str_list({"w": "abc"}, "w")
    with pytest.raises(PermanentError):
        require_str_list({"w": ["a", 2]}, "w")
    with pytest.raises(PermanentError):
        require_str_list({"w": ["a", " "]}, "w")


def test_require_choice() -> None:
    assert require_choice({"f": "ogg_opus"}, "f", ("ogg_opus",)) == "ogg_opus"
    with pytest.raises(PermanentError):
        require_choice({"f": "mp3"}, "f", ("ogg_opus",))


def test_require_out_path(tmp_path) -> None:
    target = tmp_path / "x.ogg"
    assert require_out_path({"out_path": str(target)}) == str(target)
    with pytest.raises(PermanentError):
        require_out_path({"out_path": "relative/x.ogg"})
    with pytest.raises(PermanentError):
        require_out_path({"out_path": str(tmp_path / "missing" / "x.ogg")})
