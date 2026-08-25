"""Minimal ComfyUI HTTP client: submit, poll, download.

Deliberately built on `urllib` rather than pulling in an HTTP library. The
surface is three endpoints against localhost, and a transport seam keeps the
whole client testable without a socket.

The error mapping is the point of this module. `docs/contracts/adapter-protocol.
md` requires an unreachable backend to be **permanent**, not transient: SDXL is
the last-resort image source, and a machine with no ComfyUI will not grow one by
being retried. Everything else follows the usual taxonomy.
"""

from __future__ import annotations

import json
import logging
import os
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any, Final, Protocol

from morpho_adapter_common import (
    AdapterError,
    PermanentError,
    RateLimitedError,
    TransientError,
)

log = logging.getLogger("morpho.adapter.sdxl.comfy")

BASE_URL_ENV: Final[str] = "COMFYUI_URL"
DEFAULT_BASE_URL: Final[str] = "http://127.0.0.1:8188"
TIMEOUT_ENV: Final[str] = "MORPHO_SDXL_TIMEOUT_S"
POLL_INTERVAL_ENV: Final[str] = "MORPHO_SDXL_POLL_INTERVAL_S"
CONNECT_TIMEOUT_ENV: Final[str] = "MORPHO_SDXL_CONNECT_TIMEOUT_S"

#: morphod kills the adapter at 600 s; finish first so the failure is classified.
DEFAULT_TIMEOUT_S: Final[float] = 540.0
DEFAULT_POLL_INTERVAL_S: Final[float] = 1.5
#: A reachability probe against localhost either answers immediately or is not
#: there at all. Keep it short so "not configured" is reported in milliseconds.
DEFAULT_CONNECT_TIMEOUT_S: Final[float] = 3.0

NOT_CONFIGURED_MESSAGE: Final[str] = "sdxl backend not configured"

#: Every reason a socket never reached a server — refused, no such host,
#: network unreachable, connect timeout — is an `OSError` subclass, so one
#: isinstance check covers the lot. A server that answered, however rudely,
#: comes back as an `HttpResponse` instead and never lands here.
_UNREACHABLE: Final[type[BaseException]] = OSError


@dataclass(frozen=True)
class HttpResponse:
    status: int
    body: bytes
    headers: Mapping[str, str] = field(default_factory=dict)

    def json(self) -> Any:
        try:
            return json.loads(self.body.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise TransientError(f"ComfyUI returned a non-JSON body: {exc}") from exc


class Transport(Protocol):
    """Seam for tests; the production implementation is `UrllibTransport`.

    A transport is dumb I/O: it returns an `HttpResponse` for anything the
    server answered — including 4xx and 5xx — and raises the underlying `OSError`
    / `URLError` when the socket never got there. Turning those into taxonomy
    errors is `ComfyClient`'s job, so every transport gets the same mapping.
    """

    def request(
        self,
        method: str,
        url: str,
        *,
        body: bytes | None = None,
        headers: Mapping[str, str] | None = None,
        timeout: float = 30.0,
    ) -> HttpResponse: ...


class BackendUnreachableError(PermanentError):
    """No ComfyUI answered. Reported verbatim per the adapter contract."""

    def __init__(self, detail: str = "") -> None:
        message = NOT_CONFIGURED_MESSAGE
        if detail:
            message = f"{NOT_CONFIGURED_MESSAGE}: {detail}"
        super().__init__(message)


class UrllibTransport:
    def request(
        self,
        method: str,
        url: str,
        *,
        body: bytes | None = None,
        headers: Mapping[str, str] | None = None,
        timeout: float = 30.0,
    ) -> HttpResponse:
        request = urllib.request.Request(url, data=body, method=method)
        for key, value in (headers or {}).items():
            request.add_header(key, value)
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return HttpResponse(
                    status=response.status,
                    body=response.read(),
                    headers={key.lower(): value for key, value in response.headers.items()},
                )
        except urllib.error.HTTPError as exc:
            return HttpResponse(
                status=exc.code,
                body=exc.read(),
                headers={key.lower(): value for key, value in (exc.headers or {}).items()},
            )


def classify_transport_error(exc: BaseException) -> AdapterError:
    """Map a raw socket failure onto the taxonomy.

    An unreachable ComfyUI is `permanent` by contract: SDXL is the last-resort
    image source, so a machine without a backend must land in dead letters
    rather than retry-looping forever.
    """
    reason = exc.reason if isinstance(exc, urllib.error.URLError) else exc
    if isinstance(reason, _UNREACHABLE):
        return BackendUnreachableError(str(reason))
    return TransientError(f"ComfyUI request failed: {reason}")


def base_url() -> str:
    return (os.environ.get(BASE_URL_ENV) or DEFAULT_BASE_URL).rstrip("/")


class ComfyClient:
    def __init__(
        self,
        *,
        url: str | None = None,
        transport: Transport | None = None,
        client_id: str | None = None,
    ) -> None:
        self.base_url = (url or base_url()).rstrip("/")
        self.transport = transport or UrllibTransport()
        self.client_id = client_id or uuid.uuid4().hex

    # --- endpoints ----------------------------------------------------------

    def probe(self) -> None:
        """Confirm something is listening before spending a render on it.

        Any HTTP answer counts as configured, including a 404 from a ComfyUI old
        enough to lack `/system_stats`. Only a dead socket is "not configured".
        """
        response = self._call("GET", "/system_stats", timeout=_connect_timeout())
        log.debug("ComfyUI probe returned HTTP %d", response.status)

    def submit(self, graph: Mapping[str, Any]) -> str:
        payload = json.dumps({"prompt": graph, "client_id": self.client_id}).encode("utf-8")
        response = self._call(
            "POST",
            "/prompt",
            body=payload,
            headers={"Content-Type": "application/json"},
            timeout=_connect_timeout() * 10,
        )
        _raise_for_status(response, "submitting the workflow")
        body = response.json()
        prompt_id = body.get("prompt_id") if isinstance(body, dict) else None
        if not isinstance(prompt_id, str) or not prompt_id:
            raise TransientError(f"ComfyUI accepted the workflow without a prompt_id: {body!r}")
        log.info("submitted prompt %s", prompt_id)
        return prompt_id

    def await_images(self, prompt_id: str, output_node: str) -> list[dict[str, Any]]:
        """Poll `/history` until the prompt finishes, then return its images."""
        deadline = time.monotonic() + _timeout()
        interval = _poll_interval()
        polls = 0
        while True:
            entry = self._history_entry(prompt_id)
            polls += 1
            if entry is not None:
                _raise_for_execution_error(entry, prompt_id)
                images = _extract_images(entry, output_node)
                if images:
                    log.info(
                        "prompt %s produced %d image(s) after %d polls",
                        prompt_id,
                        len(images),
                        polls,
                    )
                    return images
                if _is_finished(entry):
                    raise PermanentError(
                        f"ComfyUI finished prompt {prompt_id} without an image on node "
                        f"{output_node!r}; check the workflow's output node"
                    )
            if time.monotonic() >= deadline:
                raise TransientError(
                    f"ComfyUI did not finish prompt {prompt_id} within {_timeout():g}s"
                )
            time.sleep(min(interval, max(0.0, deadline - time.monotonic())))

    def download(self, image: Mapping[str, Any]) -> bytes:
        query = urllib.parse.urlencode(
            {
                "filename": str(image.get("filename", "")),
                "subfolder": str(image.get("subfolder", "")),
                "type": str(image.get("type", "output")),
            }
        )
        response = self._call("GET", f"/view?{query}", timeout=_connect_timeout() * 10)
        _raise_for_status(response, "downloading the render")
        if not response.body:
            raise TransientError("ComfyUI returned an empty image body")
        return response.body

    # --- plumbing -----------------------------------------------------------

    def _history_entry(self, prompt_id: str) -> dict[str, Any] | None:
        response = self._call("GET", f"/history/{urllib.parse.quote(prompt_id)}", timeout=30.0)
        if response.status == 404:
            return None
        _raise_for_status(response, "polling history")
        body = response.json()
        if not isinstance(body, dict):
            return None
        entry = body.get(prompt_id)
        return entry if isinstance(entry, dict) else None

    def _call(
        self,
        method: str,
        path: str,
        *,
        body: bytes | None = None,
        headers: Mapping[str, str] | None = None,
        timeout: float = 30.0,
    ) -> HttpResponse:
        try:
            return self.transport.request(
                method, f"{self.base_url}{path}", body=body, headers=headers, timeout=timeout
            )
        except AdapterError:
            raise
        except (urllib.error.URLError, OSError) as exc:
            raise classify_transport_error(exc) from exc


def _raise_for_status(response: HttpResponse, what: str) -> None:
    if 200 <= response.status < 300:
        return
    detail = _short(response.body)
    if response.status == 429:
        raise RateLimitedError(
            f"ComfyUI rate limited while {what}: {detail}", retry_after_ms=_retry_after(response)
        )
    if response.status in {400, 404, 422}:
        # ComfyUI answers 400 with `node_errors` when the graph references a
        # checkpoint or node the install does not have — a configuration fault,
        # not a blip.
        raise PermanentError(
            f"ComfyUI rejected the request while {what} (HTTP {response.status}): {detail}"
        )
    if 400 <= response.status < 500:
        raise PermanentError(f"ComfyUI refused while {what} (HTTP {response.status}): {detail}")
    raise TransientError(f"ComfyUI failed while {what} (HTTP {response.status}): {detail}")


def _raise_for_execution_error(entry: Mapping[str, Any], prompt_id: str) -> None:
    status = entry.get("status")
    if not isinstance(status, dict):
        return
    if status.get("status_str") != "error":
        return
    detail = ""
    messages = status.get("messages")
    if isinstance(messages, list):
        for message in messages:
            if isinstance(message, list) and message and message[0] == "execution_error":
                detail = _short(json.dumps(message[1:]).encode("utf-8"))
                break
    raise PermanentError(
        f"ComfyUI failed to execute prompt {prompt_id}" + (f": {detail}" if detail else "")
    )


def _extract_images(entry: Mapping[str, Any], output_node: str) -> list[dict[str, Any]]:
    outputs = entry.get("outputs")
    if not isinstance(outputs, dict):
        return []
    node = outputs.get(output_node)
    if not isinstance(node, dict):
        # A renamed output node should not lose the render; take any node that
        # produced images, in node order, so the choice stays deterministic.
        for key in sorted(outputs):
            candidate = outputs[key]
            if isinstance(candidate, dict) and isinstance(candidate.get("images"), list):
                node = candidate
                break
        else:
            return []
    images = node.get("images")
    if not isinstance(images, list):
        return []
    return [image for image in images if isinstance(image, dict) and image.get("filename")]


def _is_finished(entry: Mapping[str, Any]) -> bool:
    status = entry.get("status")
    if isinstance(status, dict) and "completed" in status:
        return bool(status["completed"])
    return bool(entry.get("outputs"))


def _retry_after(response: HttpResponse) -> int:
    raw = response.headers.get("retry-after") if response.headers else None
    if not raw:
        return 60_000
    try:
        seconds = int(float(raw))
    except ValueError:
        return 60_000
    return max(1, min(seconds, 3600)) * 1000


def _short(body: bytes, limit: int = 300) -> str:
    text = " ".join(body.decode("utf-8", "replace").split())
    return text if len(text) <= limit else text[: limit - 1] + "..."


def _float_env(name: str, default: float) -> float:
    raw = os.environ.get(name)
    if not raw:
        return default
    try:
        value = float(raw)
    except ValueError:
        log.warning("ignoring non-numeric %s=%r", name, raw)
        return default
    return value if value > 0 else default


def _timeout() -> float:
    return _float_env(TIMEOUT_ENV, DEFAULT_TIMEOUT_S)


def _poll_interval() -> float:
    return _float_env(POLL_INTERVAL_ENV, DEFAULT_POLL_INTERVAL_S)


def _connect_timeout() -> float:
    return _float_env(CONNECT_TIMEOUT_ENV, DEFAULT_CONNECT_TIMEOUT_S)
