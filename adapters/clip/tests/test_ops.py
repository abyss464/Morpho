"""The subprocess op handler, without a model.

``OpenClipScorer`` is monkeypatched with a stub so the suite runs without torch,
open_clip or any accelerator — the same testing seam the sidecar tests use.
"""

from __future__ import annotations

from pathlib import Path
from unittest.mock import patch

import pytest

from morpho_clip.ops import MAX_IMAGES, MAX_TEXT_CHARS, OP_SCORE, OPS, score
from morpho_clip.scorer import ALGO_VER

FIRST = "1" * 64
SECOND = "2" * 64
ABSENT = "3" * 64


class StubScorer:
    """Answers a fixed cosine per file name."""

    def __init__(self, values: dict[str, float] | None = None) -> None:
        self.values = values or {}
        self.calls: list[tuple[str, list[str]]] = []

    @property
    def model(self) -> str:
        return "ViT-B-32/laion2b_s34b_b79k"

    def score(self, text, paths):
        self.calls.append((text, [path.stem for path in paths]))
        return [self.values.get(path.stem, 0.2) for path in paths]


@pytest.fixture()
def media(tmp_path):
    for file_hash in (FIRST, SECOND):
        shard = tmp_path / file_hash[:2]
        shard.mkdir(exist_ok=True)
        (shard / f"{file_hash}.webp").write_bytes(b"bytes")
    return tmp_path


def _patch_scorer(media: Path, values: dict[str, float] | None = None):
    """Return a context manager that replaces OpenClipScorer with a stub."""
    stub = StubScorer(values)

    def fake_init(self, **_kwargs):
        self._stub = stub

    def fake_model(self):
        return stub.model

    def fake_score(self, text, paths):
        return stub.score(text, paths)

    return (
        stub,
        patch.multiple(
            "morpho_clip.ops.OpenClipScorer",
            __init__=fake_init,
            model=property(fake_model),
            score=fake_score,
        ),
    )


def test_the_op_table_has_the_right_entry():
    assert OP_SCORE in OPS
    assert OPS[OP_SCORE] is score


def test_scores_come_back_in_the_order_asked(media):
    stub, patcher = _patch_scorer(media, {FIRST: 0.31, SECOND: 0.07})
    with patcher:
        result = score({"text": "A serene lake.", "images": [SECOND, FIRST], "media_root": str(media)})
    assert [entry["file_hash"] for entry in result["scores"]] == [SECOND, FIRST]
    assert result["scores"][0]["similarity"] == pytest.approx(0.07)
    assert result["scores"][1]["similarity"] == pytest.approx(0.31)
    assert result["missing"] == []
    assert result["algo_ver"] == ALGO_VER
    assert result["model"] == "ViT-B-32/laion2b_s34b_b79k"


def test_the_identity_rides_on_the_reply(media):
    _, patcher = _patch_scorer(media)
    with patcher:
        result = score({"text": "text", "images": [FIRST], "media_root": str(media)})
    assert f"{result['algo_ver']}:{result['model']}" == "clip/1:ViT-B-32/laion2b_s34b_b79k"


def test_a_missing_picture_is_reported_not_fatal(media):
    _, patcher = _patch_scorer(media, {FIRST: 0.25})
    with patcher:
        result = score({"text": "text", "images": [FIRST, ABSENT], "media_root": str(media)})
    assert [entry["file_hash"] for entry in result["scores"]] == [FIRST]
    assert result["missing"] == [ABSENT]


def test_an_all_missing_request_never_loads_the_model(media):
    """When every hash is absent, the model is not loaded and the identity is
    still reported from the configured arch/pretrained."""
    result = score({"text": "text", "images": [ABSENT], "media_root": str(media)})
    assert result["scores"] == []
    assert result["missing"] == [ABSENT]
    assert result["algo_ver"] == ALGO_VER
    assert "ViT-B-32" in result["model"]


def test_an_empty_pool_reports_identity_without_loading(media):
    result = score({"text": "text", "images": [], "media_root": str(media)})
    assert result["scores"] == []
    assert result["missing"] == []
    assert result["algo_ver"] == ALGO_VER


def test_the_query_reaches_the_scorer_verbatim(media):
    stub, patcher = _patch_scorer(media)
    with patcher:
        score({"text": "  A serene lake.  ", "images": [FIRST], "media_root": str(media)})
    assert stub.calls[0][0] == "  A serene lake.  "


def test_missing_text_is_permanent(media):
    from morpho_adapter_common import PermanentError

    with pytest.raises(PermanentError):
        score({"images": [FIRST], "media_root": str(media)})


def test_empty_text_is_permanent(media):
    from morpho_adapter_common import PermanentError

    with pytest.raises(PermanentError):
        score({"text": "   ", "images": [FIRST], "media_root": str(media)})


def test_missing_media_root_is_permanent(media):
    from morpho_adapter_common import PermanentError

    with pytest.raises(PermanentError):
        score({"text": "text", "images": [FIRST]})


def test_an_essay_is_not_a_sentence(media):
    from morpho_adapter_common import PermanentError

    with pytest.raises(PermanentError):
        score({"text": "x" * (MAX_TEXT_CHARS + 1), "images": [FIRST], "media_root": str(media)})


def test_an_oversized_batch_is_refused(media):
    from morpho_adapter_common import PermanentError

    with pytest.raises(PermanentError):
        score({"text": "text", "images": [FIRST] * (MAX_IMAGES + 1), "media_root": str(media)})
