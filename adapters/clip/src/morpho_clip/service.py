"""The HTTP surface: two endpoints, no framework.

`http.server` rather than a web framework because the whole service is a request
shape, a model call and a JSON reply — and because it has to run under a venv
this repository does not own (the one holding the accelerator build of torch),
where every dependency added here is a dependency somebody has to install into
somebody else's environment.

The contract is `docs/contracts/clip-service.md`.
"""

from __future__ import annotations

import json
import logging
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Final

from .media import MediaLibrary
from .scorer import ALGO_VER, Scorer

log = logging.getLogger("morpho.clip.service")

#: Mirrors `morpho_reconcile::sources::clip::MAX_IMAGES_PER_REQUEST`. The engine
#: refuses to send more; this refuses to accept more, so a client that has not
#: been updated gets a clear 400 rather than a request that runs for a minute.
MAX_IMAGES: Final[int] = 64
#: A sentence, not an essay. Longer than this and the caller is sending
#: something other than a slot-1 example.
MAX_TEXT_CHARS: Final[int] = 2_000
#: Enough for the biggest legitimate request several times over.
MAX_BODY_BYTES: Final[int] = 1 << 20


class ScoreService:
    """Request handling, with no HTTP in it.

    Kept separate from the handler so the tests can exercise every branch by
    calling a method, and so the transport could be replaced without touching
    any of the logic that matters.
    """

    def __init__(self, scorer: Scorer, library: MediaLibrary) -> None:
        self.scorer = scorer
        self.library = library

    def health(self) -> dict[str, Any]:
        return {
            "ok": True,
            "algo_ver": ALGO_VER,
            "model": self.scorer.model,
            "media_root": str(self.library.root),
        }

    def score(self, payload: Any) -> dict[str, Any]:
        """Answer one scoring request, or raise [`BadRequestError`].

        Pictures the library cannot produce are reported in `missing` rather
        than failing the request: one file lost to a restore must not cost a
        word the scores of its other candidates, and the engine logs the count
        and carries on.
        """
        if not isinstance(payload, dict):
            raise BadRequestError("body must be a JSON object")
        text = payload.get("text")
        if not isinstance(text, str) or not text.strip():
            raise BadRequestError("`text` must be a non-empty string")
        if len(text) > MAX_TEXT_CHARS:
            raise BadRequestError(f"`text` exceeds {MAX_TEXT_CHARS} characters")
        images = payload.get("images")
        if not isinstance(images, list) or not all(isinstance(item, str) for item in images):
            raise BadRequestError("`images` must be a list of content hashes")
        if len(images) > MAX_IMAGES:
            raise BadRequestError(f"`images` exceeds {MAX_IMAGES} entries")

        resolved: list[tuple[str, Path]] = []
        missing: list[str] = []
        for file_hash in images:
            path = self.library.path_for(file_hash)
            if path is None:
                missing.append(file_hash)
            else:
                resolved.append((file_hash, path))

        similarities = self.scorer.score(text, [path for _, path in resolved]) if resolved else []
        if len(similarities) != len(resolved):
            # A backend that answers with the wrong number of scores has broken
            # the one invariant that makes the reply readable: position.
            raise RuntimeError(
                f"scorer returned {len(similarities)} scores for {len(resolved)} pictures"
            )
        return {
            "model": self.scorer.model,
            "algo_ver": ALGO_VER,
            "scores": [
                {"file_hash": file_hash, "similarity": similarity}
                for (file_hash, _), similarity in zip(resolved, similarities, strict=True)
            ],
            "missing": missing,
        }


class BadRequestError(Exception):
    """The request was readable and wrong. Answered 400, never retried."""


def build_handler(service: ScoreService) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        server_version = "morpho-clip/1"
        protocol_version = "HTTP/1.1"

        def do_GET(self) -> None:
            if self.path.rstrip("/") in {"", "/health"}:
                self._reply(200, service.health())
            else:
                self._reply(404, {"error": "no such endpoint"})

        def do_POST(self) -> None:
            if self.path.rstrip("/") != "/score":
                self._reply(404, {"error": "no such endpoint"})
                return
            try:
                payload = self._read_json()
                self._reply(200, service.score(payload))
            except BadRequestError as exc:
                self._reply(400, {"error": str(exc)})
            except Exception as exc:
                log.exception("scoring failed")
                self._reply(500, {"error": str(exc)})

        def _read_json(self) -> Any:
            try:
                length = int(self.headers.get("Content-Length", "0"))
            except ValueError as exc:
                raise BadRequestError("Content-Length is not a number") from exc
            if length <= 0:
                raise BadRequestError("empty body")
            if length > MAX_BODY_BYTES:
                raise BadRequestError(f"body exceeds {MAX_BODY_BYTES} bytes")
            try:
                return json.loads(self.rfile.read(length))
            except (UnicodeDecodeError, json.JSONDecodeError) as exc:
                raise BadRequestError(f"body is not JSON: {exc}") from exc

        def _reply(self, status: int, body: dict[str, Any]) -> None:
            encoded = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

        def log_message(self, fmt: str, *args: Any) -> None:
            # The default writes to stderr unconditionally; route it through the
            # logger so `--quiet` means something.
            log.debug(fmt, *args)

    return Handler


def serve(service: ScoreService, host: str, port: int) -> None:
    """Run until interrupted."""
    server = ThreadingHTTPServer((host, port), build_handler(service))
    log.info("clip sidecar on http://%s:%d (%s)", host, port, service.scorer.model)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        log.info("stopping")
    finally:
        server.server_close()
