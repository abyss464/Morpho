"""Shared fixtures. Nothing here touches the network."""

from __future__ import annotations

import random
from pathlib import Path

import morfessor
import pytest

from morpho_morfessor import model as model_mod

PREFIXES = (
    "un",
    "re",
    "in",
    "dis",
    "pre",
    "non",
    "over",
    "under",
    "mis",
    "sub",
    "inter",
    "trans",
)
ROOTS = (
    "happi",
    "nation",
    "cover",
    "form",
    "port",
    "duct",
    "struct",
    "script",
    "spect",
    "dict",
)
SUFFIXES = ("ness", "ate", "ion", "able", "ive", "ment")


def synthetic_vocabulary(count: int) -> list[str]:
    """A deterministic affixed vocabulary: enough signal for Morfessor to learn."""
    words = sorted({p + r + s for p in PREFIXES for r in ROOTS for s in SUFFIXES})
    if count > len(words):
        raise ValueError(f"only {len(words)} synthetic words available")
    return words[:count]


@pytest.fixture(scope="session")
def trainable_batch() -> list[str]:
    """Comfortably above the ad-hoc vocabulary floor, small enough to stay fast."""
    return synthetic_vocabulary(120)


@pytest.fixture(autouse=True)
def isolated_model_dir(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> Path:
    """Never let a model the developer happens to have on disk leak into a test."""
    monkeypatch.delenv(model_mod.MODEL_PATH_ENV, raising=False)
    monkeypatch.delenv(model_mod.MAX_EPOCHS_ENV, raising=False)
    monkeypatch.delenv(model_mod.MORPH_LENGTH_ENV, raising=False)
    monkeypatch.delenv(model_mod.MIN_VOCAB_ENV, raising=False)
    empty = tmp_path / "model"
    empty.mkdir()
    monkeypatch.setattr(model_mod, "MODEL_DIR", empty)
    return empty


@pytest.fixture(scope="session")
def pretrained_model_file(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """A real Morfessor binary model, trained once and reused across tests."""
    directory = tmp_path_factory.mktemp("pretrained")
    random.seed(model_mod.TRAINING_SEED)
    model = morfessor.BaselineModel(
        use_skips=False,
        corpusweight=morfessor.MorphLengthCorpusWeight(model_mod.DEFAULT_MORPH_LENGTH),
    )
    model.load_data([(1, word) for word in synthetic_vocabulary(120)])
    model.train_batch(algorithm="recursive", max_epochs=4)
    path = directory / "baseline.bin"
    morfessor.MorfessorIO().write_binary_model_file(str(path), model)
    return path
