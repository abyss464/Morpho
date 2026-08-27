"""The transport, end to end, against a real socket.

Everything else is tested by calling `ScoreService` directly; this exists to
prove that the bytes on the wire are the bytes `morpho_reconcile::sources::clip`
expects — status codes included, since the engine's taxonomy turns a 400 into a
permanent failure and a 500 into a retry.
"""

from __future__ import annotations

import json
import threading
import urllib.error
import urllib.request
from http.server import ThreadingHTTPServer

import pytest

from morpho_clip.media import MediaLibrary
from morpho_clip.service import ScoreService, build_handler

FIRST = "1" * 64


class StubScorer:
    @property
    def model(self) -> str:
        return "ViT-B-32/laion2b_s34b_b79k"

    def score(self, text, paths):
        return [0.25 for _ in paths]


@pytest.fixture
def base_url(tmp_path):
    shard = tmp_path / FIRST[:2]
    shard.mkdir()
    (shard / f"{FIRST}.webp").write_bytes(b"bytes")
    service = ScoreService(StubScorer(), MediaLibrary(tmp_path))
    server = ThreadingHTTPServer(("127.0.0.1", 0), build_handler(service))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_address[1]}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def post(url, payload):
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        return response.status, json.load(response)


def test_health_answers_over_http(base_url):
    with urllib.request.urlopen(f"{base_url}/health", timeout=10) as response:
        body = json.load(response)
    assert body["ok"] is True
    assert body["algo_ver"] == "clip/1"


def test_scoring_answers_the_contract_shape(base_url):
    status, body = post(f"{base_url}/score", {"text": "A serene lake.", "images": [FIRST]})
    assert status == 200
    assert body["scores"] == [{"file_hash": FIRST, "similarity": 0.25}]
    assert body["missing"] == []
    assert set(body) == {"model", "algo_ver", "scores", "missing"}


def test_a_malformed_body_is_a_400(base_url):
    """The engine maps 4xx onto a permanent failure, which is right: a request
    this service cannot read will not become readable on a retry."""
    with pytest.raises(urllib.error.HTTPError) as caught:
        post(f"{base_url}/score", {"images": [FIRST]})
    assert caught.value.code == 400


def test_an_unknown_endpoint_is_a_404(base_url):
    with pytest.raises(urllib.error.HTTPError) as caught:
        post(f"{base_url}/embed", {"text": "x", "images": []})
    assert caught.value.code == 404
