"""The request shape, the reply shape, and everything that can go wrong.

No model is loaded anywhere in this file: `StubScorer` stands in for it, which
is the point of the `Scorer` seam. The suite therefore runs on a machine with no
accelerator and no weights — including this repository's CI, which has neither.
"""

from __future__ import annotations

import json

import pytest

from morpho_clip.media import MediaLibrary
from morpho_clip.scorer import ALGO_VER
from morpho_clip.service import MAX_IMAGES, BadRequestError, ScoreService

FIRST = "1" * 64
SECOND = "2" * 64
ABSENT = "3" * 64


class StubScorer:
    """Answers a fixed cosine per file name, and records what it was asked."""

    def __init__(self, values: dict[str, float] | None = None) -> None:
        self.values = values or {}
        self.calls: list[tuple[str, list[str]]] = []

    @property
    def model(self) -> str:
        return "ViT-B-32/laion2b_s34b_b79k"

    def score(self, text, paths):
        self.calls.append((text, [path.stem for path in paths]))
        return [self.values.get(path.stem, 0.2) for path in paths]


@pytest.fixture
def service(tmp_path):
    for file_hash in (FIRST, SECOND):
        shard = tmp_path / file_hash[:2]
        shard.mkdir(exist_ok=True)
        (shard / f"{file_hash}.webp").write_bytes(b"bytes")
    scorer = StubScorer({FIRST: 0.31, SECOND: 0.07})
    return ScoreService(scorer, MediaLibrary(tmp_path))


def test_health_names_the_identity_the_engine_checks(service):
    body = service.health()
    assert body["ok"] is True
    assert body["algo_ver"] == ALGO_VER
    assert body["model"] == "ViT-B-32/laion2b_s34b_b79k"


def test_scores_come_back_in_the_order_they_were_asked_for(service):
    body = service.score({"text": "A serene lake.", "images": [SECOND, FIRST]})
    assert [entry["file_hash"] for entry in body["scores"]] == [SECOND, FIRST]
    assert body["scores"][0]["similarity"] == pytest.approx(0.07)
    assert body["scores"][1]["similarity"] == pytest.approx(0.31)
    assert body["missing"] == []
    assert body["algo_ver"] == ALGO_VER


def test_the_identity_rides_on_every_reply(service):
    """The engine refuses scores whose identity is not the one it stores under,
    so the reply has to carry it — a bare list of numbers would be unfileable."""
    body = service.score({"text": "text", "images": [FIRST]})
    assert f"{body['algo_ver']}:{body['model']}" == "clip/1:ViT-B-32/laion2b_s34b_b79k"


def test_a_picture_the_library_has_lost_is_reported_not_fatal(service):
    """One missing file must not cost the word its other candidates' scores."""
    body = service.score({"text": "text", "images": [FIRST, ABSENT]})
    assert [entry["file_hash"] for entry in body["scores"]] == [FIRST]
    assert body["missing"] == [ABSENT]


def test_an_all_missing_request_never_reaches_the_model(service):
    body = service.score({"text": "text", "images": [ABSENT]})
    assert body["scores"] == []
    assert body["missing"] == [ABSENT]
    assert service.scorer.calls == [], "no pictures, no model call"


def test_an_empty_pool_is_answered_without_a_model_call(service):
    body = service.score({"text": "text", "images": []})
    assert body["scores"] == []
    assert service.scorer.calls == []


@pytest.mark.parametrize(
    "payload",
    [
        [],
        "a string",
        {"images": [FIRST]},
        {"text": "", "images": [FIRST]},
        {"text": "   ", "images": [FIRST]},
        {"text": 7, "images": [FIRST]},
        {"text": "text"},
        {"text": "text", "images": "not a list"},
        {"text": "text", "images": [1, 2]},
    ],
)
def test_a_malformed_request_is_a_bad_request(service, payload):
    with pytest.raises(BadRequestError):
        service.score(payload)


def test_an_oversized_batch_is_refused(service):
    with pytest.raises(BadRequestError):
        service.score({"text": "text", "images": [FIRST] * (MAX_IMAGES + 1)})


def test_an_essay_is_not_a_sentence(service):
    with pytest.raises(BadRequestError):
        service.score({"text": "x" * 5_000, "images": [FIRST]})


def test_the_query_reaches_the_model_verbatim(service):
    """The engine hashes the text it sent and files the answer under that hash.
    Anything this side did to the text — trimming, casing — would file the score
    under a query nobody asked."""
    service.score({"text": "  A serene lake.  ", "images": [FIRST]})
    assert service.scorer.calls[0][0] == "  A serene lake.  "


def test_a_backend_that_miscounts_is_a_server_error(tmp_path):
    """Position is the only thing tying a score to a picture, so a backend that
    answers with the wrong number of them has broken the reply, not one entry."""

    class Miscounting(StubScorer):
        def score(self, text, paths):
            return [0.1]

    shard = tmp_path / FIRST[:2]
    shard.mkdir()
    (shard / f"{FIRST}.webp").write_bytes(b"bytes")
    (tmp_path / SECOND[:2]).mkdir()
    (tmp_path / SECOND[:2] / f"{SECOND}.webp").write_bytes(b"bytes")
    service = ScoreService(Miscounting(), MediaLibrary(tmp_path))
    with pytest.raises(RuntimeError):
        service.score({"text": "text", "images": [FIRST, SECOND]})


def test_the_reply_is_json_serializable(service):
    """It goes onto a socket as JSON; a numpy float would not."""
    body = service.score({"text": "text", "images": [FIRST]})
    assert json.loads(json.dumps(body)) == body
