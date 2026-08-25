"""The contract asks for idempotency: same request, same bytes out.

Morfessor's batch training shuffles its compound list with the global `random`
module every epoch, so without a fixed seed two runs of the same batch disagree.
These tests are the guard on that.
"""

from __future__ import annotations

import random

import pytest

from morpho_morfessor.ops import segment


def test_same_batch_twice_is_identical(trainable_batch: list[str]) -> None:
    first = segment({"words": trainable_batch})
    second = segment({"words": trainable_batch})
    assert first == second


def test_a_hostile_global_random_state_cannot_perturb_the_result(
    trainable_batch: list[str],
) -> None:
    random.seed(1)
    first = segment({"words": trainable_batch})
    random.seed(999_999)
    [random.random() for _ in range(1000)]
    second = segment({"words": trainable_batch})
    assert first == second


def test_batch_order_does_not_change_segmentation(trainable_batch: list[str]) -> None:
    forward = segment({"words": trainable_batch})
    shuffled = list(trainable_batch)
    random.Random(4).shuffle(shuffled)
    backward = segment({"words": shuffled})
    assert backward["model_ver"] == forward["model_ver"]
    assert backward["segments"] == forward["segments"]


def test_duplicates_collapse_without_changing_anything(trainable_batch: list[str]) -> None:
    plain = segment({"words": trainable_batch})
    doubled = segment({"words": trainable_batch + trainable_batch})
    assert doubled == plain


def test_a_different_vocabulary_gets_a_different_model_ver(trainable_batch: list[str]) -> None:
    from conftest import synthetic_vocabulary

    first = segment({"words": trainable_batch})
    second = segment({"words": synthetic_vocabulary(140)})
    assert second["model_ver"] != first["model_ver"], (
        "an ad-hoc model trained on other words must not claim the same version"
    )


def test_hyperparameters_are_part_of_the_model_ver(
    trainable_batch: list[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    from morpho_morfessor import model as model_mod

    baseline = segment({"words": trainable_batch})["model_ver"]
    monkeypatch.setenv(model_mod.MORPH_LENGTH_ENV, "3.0")
    assert segment({"words": trainable_batch})["model_ver"] != baseline
    monkeypatch.delenv(model_mod.MORPH_LENGTH_ENV)
    monkeypatch.setenv(model_mod.MAX_EPOCHS_ENV, "2")
    assert segment({"words": trainable_batch})["model_ver"] != baseline


def test_segments_always_reassemble_to_the_input(trainable_batch: list[str]) -> None:
    result = segment({"words": trainable_batch})
    for word, parts in result["segments"].items():
        assert "".join(parts) == word
        assert all(parts), "no empty morphs"


def test_every_requested_word_is_answered(trainable_batch: list[str]) -> None:
    result = segment({"words": trainable_batch})
    assert set(result["segments"]) == set(trainable_batch)
