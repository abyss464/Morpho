"""Op-level validation and the response shape the contract specifies."""

from __future__ import annotations

import pytest
from morpho_adapter_common import PermanentError, dispatch

from morpho_morfessor.ops import MAX_BATCH, MAX_WORD_CHARS, OPS, segment


def test_contract_example_shape() -> None:
    result = segment({"words": ["benevolent", "unhappiness"]})
    assert set(result) == {"segments", "model_ver"}
    assert set(result["segments"]) == {"benevolent", "unhappiness"}
    assert all(isinstance(parts, list) for parts in result["segments"].values())
    assert result["model_ver"].startswith("morfessor/")


def test_response_envelope() -> None:
    response = dispatch(OPS, "morfessor.segment", {"words": ["benevolent"]})
    assert response["ok"] is True
    assert response["result"]["segments"] == {"benevolent": ["benevolent"]}


def test_words_are_deduplicated_preserving_first_seen_order() -> None:
    result = segment({"words": ["beta", "alpha", "beta"]})
    assert list(result["segments"]) == ["beta", "alpha"]


def test_surrounding_whitespace_is_stripped() -> None:
    result = segment({"words": ["  benevolent\n"]})
    assert list(result["segments"]) == ["benevolent"]


def test_case_is_preserved() -> None:
    result = segment({"words": ["Benevolent"]})
    assert list(result["segments"]) == ["Benevolent"]


@pytest.mark.parametrize(
    "params",
    [
        {},
        {"words": []},
        {"words": "benevolent"},
        {"words": ["benevolent", 7]},
        {"words": ["benevolent", ""]},
        {"words": ["   "]},
        {"words": [None]},
    ],
)
def test_bad_params_are_permanent(params: dict[str, object]) -> None:
    with pytest.raises(PermanentError):
        segment(params)


def test_oversized_batch_is_permanent() -> None:
    with pytest.raises(PermanentError, match="exceeds"):
        segment({"words": [f"word{index}" for index in range(MAX_BATCH + 1)]})


def test_oversized_word_is_permanent() -> None:
    with pytest.raises(PermanentError, match="characters"):
        segment({"words": ["a" * (MAX_WORD_CHARS + 1)]})


def test_word_at_the_length_limit_is_accepted() -> None:
    word = "a" * MAX_WORD_CHARS
    assert list(segment({"words": [word]})["segments"]) == [word]


def test_non_ascii_words_survive() -> None:
    result = segment({"words": ["naïve", "café"]})
    assert result["segments"] == {"naïve": ["naïve"], "café": ["café"]}
