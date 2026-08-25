"""Model discovery, versioning, and the two ad-hoc quality guards."""

from __future__ import annotations

import shutil
from pathlib import Path

import pytest
from morpho_adapter_common import PermanentError

from conftest import synthetic_vocabulary
from morpho_morfessor import model as model_mod

# --- discovery --------------------------------------------------------------


def test_empty_directory_means_no_model() -> None:
    assert model_mod.find_model_file() is None


def test_missing_directory_means_no_model(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    monkeypatch.setattr(model_mod, "MODEL_DIR", tmp_path / "absent")
    assert model_mod.find_model_file() is None


def test_readme_and_gitignore_are_not_models(isolated_model_dir: Path) -> None:
    (isolated_model_dir / "README.md").write_text("docs", encoding="utf-8")
    (isolated_model_dir / ".gitignore").write_text("*.bin", encoding="utf-8")
    assert model_mod.find_model_file() is None


@pytest.mark.parametrize("name", ["baseline.bin", "corpus.model", "trained.gz"])
def test_recognised_model_extensions(isolated_model_dir: Path, name: str) -> None:
    target = isolated_model_dir / name
    target.write_bytes(b"placeholder")
    assert model_mod.find_model_file() == target


def test_lowest_sorting_model_wins(isolated_model_dir: Path) -> None:
    (isolated_model_dir / "zeta.bin").write_bytes(b"z")
    (isolated_model_dir / "alpha.bin").write_bytes(b"a")
    assert model_mod.find_model_file().name == "alpha.bin"


def test_env_override_wins(
    isolated_model_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    (isolated_model_dir / "ignored.bin").write_bytes(b"x")
    chosen = tmp_path / "elsewhere.bin"
    chosen.write_bytes(b"y")
    monkeypatch.setenv(model_mod.MODEL_PATH_ENV, str(chosen))
    assert model_mod.find_model_file() == chosen


def test_env_override_pointing_nowhere_is_permanent(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(model_mod.MODEL_PATH_ENV, "/nonexistent/model.bin")
    with pytest.raises(PermanentError, match="missing file"):
        model_mod.find_model_file()


# --- pretrained path --------------------------------------------------------


def test_pretrained_model_is_used_and_versioned(
    isolated_model_dir: Path, pretrained_model_file: Path
) -> None:
    shutil.copy(pretrained_model_file, isolated_model_dir / "baseline.bin")
    result = model_mod.segment_batch(["unhappiness", "disformment"])
    assert result.source == model_mod.SOURCE_PRETRAINED
    assert result.model_ver.startswith(f"morfessor/{model_mod.library_version()}+model-")
    assert set(result.segments) == {"unhappiness", "disformment"}
    for word, parts in result.segments.items():
        assert "".join(parts) == word


def test_version_sidecar_names_the_corpus_vintage(
    isolated_model_dir: Path, pretrained_model_file: Path
) -> None:
    shutil.copy(pretrained_model_file, isolated_model_dir / "baseline.bin")
    (isolated_model_dir / model_mod.VERSION_SIDECAR).write_text("2026-08\n", encoding="utf-8")
    result = model_mod.segment_batch(["unhappiness"])
    assert result.model_ver == f"morfessor/{model_mod.library_version()}+model-2026-08"


def test_blank_sidecar_falls_back_to_a_content_digest(
    isolated_model_dir: Path, pretrained_model_file: Path
) -> None:
    shutil.copy(pretrained_model_file, isolated_model_dir / "baseline.bin")
    (isolated_model_dir / model_mod.VERSION_SIDECAR).write_text("   \n", encoding="utf-8")
    result = model_mod.segment_batch(["unhappiness"])
    assert result.model_ver.startswith("morfessor/")
    assert "+model-" in result.model_ver
    assert "2026" not in result.model_ver


def test_a_pretrained_model_serves_a_batch_below_the_vocabulary_floor(
    isolated_model_dir: Path, pretrained_model_file: Path
) -> None:
    shutil.copy(pretrained_model_file, isolated_model_dir / "baseline.bin")
    result = model_mod.segment_batch(["unnationness"])
    assert result.source == model_mod.SOURCE_PRETRAINED
    assert len(result.segments["unnationness"]) > 1, "the floor guards ad-hoc training only"


def test_a_corrupt_model_file_is_permanent(isolated_model_dir: Path) -> None:
    (isolated_model_dir / "baseline.bin").write_bytes(b"not a pickle at all")
    with pytest.raises(PermanentError, match="cannot load"):
        model_mod.segment_batch(["unhappiness"])


# --- ad-hoc guards ----------------------------------------------------------


def test_small_batches_come_back_unsegmented() -> None:
    words = ["benevolent", "unhappiness", "abandon"]
    result = model_mod.segment_batch(words)
    assert result.source == model_mod.SOURCE_UNSEGMENTED
    assert result.segments == {word: [word] for word in words}
    assert result.model_ver.endswith(f"+{model_mod.NO_MODEL_VER_SUFFIX}")


def test_the_unsegmented_version_carries_no_batch_digest() -> None:
    first = model_mod.segment_batch(["alpha", "beta"])
    second = model_mod.segment_batch(["gamma"])
    assert first.model_ver == second.model_ver, (
        "identity segmentation is the same function whatever the batch"
    )


def test_the_vocabulary_floor_is_configurable(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(model_mod.MIN_VOCAB_ENV, "10")
    result = model_mod.segment_batch(synthetic_vocabulary(40))
    assert result.source == model_mod.SOURCE_ADHOC


def test_trained_batches_report_the_adhoc_version(trainable_batch: list[str]) -> None:
    result = model_mod.segment_batch(trainable_batch)
    assert result.source == model_mod.SOURCE_ADHOC
    assert f"+{model_mod.ADHOC_TAG_PREFIX}-" in result.model_ver


def test_a_degenerate_model_is_discarded(
    trainable_batch: list[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    # Demand implausibly long morphs; every real segmentation now looks degenerate.
    monkeypatch.setattr(model_mod, "MIN_MEAN_MORPH_CHARS", 99.0)
    result = model_mod.segment_batch(trainable_batch)
    assert result.source == model_mod.SOURCE_UNSEGMENTED
    assert all(parts == [word] for word, parts in result.segments.items())


def test_mean_morph_chars() -> None:
    assert model_mod.mean_morph_chars({}) == 0.0
    assert model_mod.mean_morph_chars({"abc": ["abc"]}) == 3.0
    assert model_mod.mean_morph_chars({"abcd": ["ab", "cd"]}) == 2.0
    assert model_mod.mean_morph_chars({"abc": ["a", "b", "c"]}) == 1.0


def test_adhoc_tag_is_stable_and_sensitive() -> None:
    words = ["alpha", "beta"]
    base = model_mod.adhoc_tag(words, 5.0, 8)
    assert base == model_mod.adhoc_tag(words, 5.0, 8)
    assert base != model_mod.adhoc_tag(["alpha", "gamma"], 5.0, 8)
    assert base != model_mod.adhoc_tag(words, 4.0, 8)
    assert base != model_mod.adhoc_tag(words, 5.0, 4)


# --- env parsing ------------------------------------------------------------


@pytest.mark.parametrize(
    ("value", "expected"),
    [(None, model_mod.DEFAULT_MAX_EPOCHS), ("3", 3), ("0", model_mod.DEFAULT_MAX_EPOCHS)],
)
def test_max_epochs_env(monkeypatch: pytest.MonkeyPatch, value: str | None, expected: int) -> None:
    if value is None:
        monkeypatch.delenv(model_mod.MAX_EPOCHS_ENV, raising=False)
    else:
        monkeypatch.setenv(model_mod.MAX_EPOCHS_ENV, value)
    assert model_mod._max_epochs() == expected


def test_bad_env_values_fall_back_to_defaults(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(model_mod.MAX_EPOCHS_ENV, "many")
    monkeypatch.setenv(model_mod.MORPH_LENGTH_ENV, "long")
    monkeypatch.setenv(model_mod.MIN_VOCAB_ENV, "-4")
    assert model_mod._max_epochs() == model_mod.DEFAULT_MAX_EPOCHS
    assert model_mod._morph_length() == model_mod.DEFAULT_MORPH_LENGTH
    assert model_mod._min_vocab() == model_mod.DEFAULT_MIN_VOCAB


def test_the_vocabulary_floor_can_be_disabled(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(model_mod.MIN_VOCAB_ENV, "0")
    assert model_mod._min_vocab() == 0
