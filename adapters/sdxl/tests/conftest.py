"""A scripted ComfyUI. Nothing in this suite opens a socket."""

from __future__ import annotations

import io
import json
import socket
from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any

import pytest
from PIL import Image

from morpho_sdxl.comfy import HttpResponse

PROMPT_ID = "0f3d7c1e-morpho-test"


def png_bytes(width: int = 1152, height: int = 896, colour: tuple[int, int, int] = (32, 96, 160)):
    """A real PNG so Pillow decodes it for real."""
    image = Image.new("RGB", (width, height), colour)
    for x in range(0, width, 8):
        for y in range(0, height, 8):
            image.putpixel((x, y), ((x * 7) % 256, (y * 5) % 256, 90))
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")
    return buffer.getvalue()


def json_response(payload: Any, status: int = 200) -> HttpResponse:
    return HttpResponse(status=status, body=json.dumps(payload).encode("utf-8"))


def history_payload(
    *,
    node: str = "9",
    filename: str = "morpho_00001_.png",
    status_str: str = "success",
    completed: bool = True,
) -> dict[str, Any]:
    return {
        PROMPT_ID: {
            "status": {"status_str": status_str, "completed": completed, "messages": []},
            "outputs": {
                node: {"images": [{"filename": filename, "subfolder": "", "type": "output"}]}
            },
        }
    }


@dataclass
class FakeTransport:
    """Routes by path prefix; records every call for assertions."""

    image: bytes = field(default_factory=png_bytes)
    history: Any = field(default_factory=history_payload)
    probe_status: int = 200
    submit_response: HttpResponse | None = None
    history_responses: list[HttpResponse] = field(default_factory=list)
    view_response: HttpResponse | None = None
    raise_on: dict[str, BaseException] = field(default_factory=dict)
    calls: list[tuple[str, str, bytes | None]] = field(default_factory=list)

    def request(
        self,
        method: str,
        url: str,
        *,
        body: bytes | None = None,
        headers: Mapping[str, str] | None = None,
        timeout: float = 30.0,
    ) -> HttpResponse:
        path = url.split("://", 1)[-1].split("/", 1)[-1]
        path = "/" + path
        self.calls.append((method, path, body))
        for marker, error in self.raise_on.items():
            if marker in path:
                raise error
        if path.startswith("/system_stats"):
            return json_response({"system": {"comfyui_version": "0.3.0"}}, self.probe_status)
        if path.startswith("/prompt"):
            return self.submit_response or json_response({"prompt_id": PROMPT_ID})
        if path.startswith("/history"):
            if self.history_responses:
                return self.history_responses.pop(0)
            return json_response(self.history)
        if path.startswith("/view"):
            return self.view_response or HttpResponse(status=200, body=self.image)
        return HttpResponse(status=404, body=b"not found")

    def paths(self) -> list[str]:
        return [path for _method, path, _body in self.calls]

    def submitted_graph(self) -> dict[str, Any]:
        for method, path, body in self.calls:
            if method == "POST" and path.startswith("/prompt") and body:
                return json.loads(body)["prompt"]
        raise AssertionError("no workflow was submitted")


@pytest.fixture
def transport(monkeypatch: pytest.MonkeyPatch) -> FakeTransport:
    """Install the fake as the default transport `ComfyClient` builds."""
    from morpho_sdxl import comfy

    fake = FakeTransport()
    monkeypatch.setattr(comfy, "UrllibTransport", lambda: fake)
    monkeypatch.setenv(comfy.BASE_URL_ENV, "http://127.0.0.1:8188")
    monkeypatch.setenv(comfy.POLL_INTERVAL_ENV, "0.01")
    return fake


@pytest.fixture(autouse=True)
def clean_env(monkeypatch: pytest.MonkeyPatch) -> None:
    from morpho_sdxl import comfy, image, workflow

    for name in (
        workflow.WORKFLOW_ENV,
        workflow.CHECKPOINT_ENV,
        image.QUALITY_ENV,
        image.NATIVE_BUCKETS_ENV,
        comfy.BASE_URL_ENV,
        comfy.TIMEOUT_ENV,
        comfy.POLL_INTERVAL_ENV,
        comfy.CONNECT_TIMEOUT_ENV,
    ):
        monkeypatch.delenv(name, raising=False)


def closed_loopback_port() -> int:
    """An ephemeral port nothing is listening on: a guaranteed refusal."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]
