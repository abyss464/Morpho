"""ComfyUI client behaviour and, above all, its error mapping."""

from __future__ import annotations

import socket
import urllib.error

import pytest
from morpho_adapter_common import PermanentError, RateLimitedError, TransientError

from conftest import PROMPT_ID, FakeTransport, history_payload, json_response
from morpho_sdxl import comfy
from morpho_sdxl.comfy import BackendUnreachableError, ComfyClient, HttpResponse


def client(transport: FakeTransport) -> ComfyClient:
    return ComfyClient(url="http://127.0.0.1:8188", transport=transport, client_id="test")


# --- configuration ----------------------------------------------------------


def test_default_base_url() -> None:
    assert comfy.base_url() == comfy.DEFAULT_BASE_URL == "http://127.0.0.1:8188"


def test_base_url_from_env(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(comfy.BASE_URL_ENV, "http://gpu-box.lan:9000/")
    assert comfy.base_url() == "http://gpu-box.lan:9000"


# --- the contract's "not configured" path ------------------------------------


@pytest.mark.parametrize(
    "reason",
    [
        ConnectionRefusedError(111, "Connection refused"),
        socket.gaierror(-2, "Name or service not known"),
        OSError(101, "Network is unreachable"),
        TimeoutError("timed out"),
    ],
)
def test_a_dead_socket_is_the_contract_message(transport: FakeTransport, reason: Exception) -> None:
    transport.raise_on = {"/system_stats": urllib.error.URLError(reason)}
    with pytest.raises(BackendUnreachableError) as excinfo:
        client(transport).probe()
    assert excinfo.value.kind == "permanent"
    assert excinfo.value.message.startswith(comfy.NOT_CONFIGURED_MESSAGE)


def test_backend_unreachable_message_without_detail() -> None:
    assert BackendUnreachableError().message == comfy.NOT_CONFIGURED_MESSAGE


def test_a_reachable_but_ancient_comfyui_still_counts_as_configured(
    transport: FakeTransport,
) -> None:
    transport.probe_status = 404
    client(transport).probe()  # any HTTP answer means something is listening


def test_probe_hits_system_stats(transport: FakeTransport) -> None:
    client(transport).probe()
    assert transport.paths() == ["/system_stats"]


# --- submit -----------------------------------------------------------------


def test_submit_posts_the_graph_and_returns_the_prompt_id(transport: FakeTransport) -> None:
    assert client(transport).submit({"1": {"class_type": "Node"}}) == PROMPT_ID
    method, path, body = transport.calls[-1]
    assert (method, path) == ("POST", "/prompt")
    assert body is not None
    assert b"client_id" in body


def test_submit_without_a_prompt_id_is_transient(transport: FakeTransport) -> None:
    transport.submit_response = json_response({"error": "nope"})
    with pytest.raises(TransientError, match="prompt_id"):
        client(transport).submit({})


def test_a_graph_comfyui_cannot_run_is_permanent(transport: FakeTransport) -> None:
    transport.submit_response = json_response(
        {"error": {"type": "prompt_outputs_failed_validation"}, "node_errors": {"4": {}}}, 400
    )
    with pytest.raises(PermanentError) as excinfo:
        client(transport).submit({})
    assert "node_errors" in excinfo.value.message


def test_a_rate_limited_submit_parks_the_lane(transport: FakeTransport) -> None:
    transport.submit_response = HttpResponse(
        status=429, body=b"slow down", headers={"retry-after": "42"}
    )
    with pytest.raises(RateLimitedError) as excinfo:
        client(transport).submit({})
    assert excinfo.value.retry_after_ms == 42_000


def test_a_rate_limited_submit_without_a_header_uses_a_default(transport: FakeTransport) -> None:
    transport.submit_response = HttpResponse(status=429, body=b"slow down")
    with pytest.raises(RateLimitedError) as excinfo:
        client(transport).submit({})
    assert excinfo.value.retry_after_ms == 60_000


def test_a_server_error_is_transient(transport: FakeTransport) -> None:
    transport.submit_response = HttpResponse(status=503, body=b"backend restarting")
    with pytest.raises(TransientError):
        client(transport).submit({})


def test_a_non_json_body_is_transient(transport: FakeTransport) -> None:
    transport.submit_response = HttpResponse(status=200, body=b"<html>hello</html>")
    with pytest.raises(TransientError, match="non-JSON"):
        client(transport).submit({})


# --- polling ----------------------------------------------------------------


def test_await_images_polls_until_history_appears(transport: FakeTransport) -> None:
    transport.history_responses = [
        HttpResponse(status=404, body=b""),
        json_response({}),
        json_response(history_payload()),
    ]
    images = client(transport).await_images(PROMPT_ID, "9")
    assert images[0]["filename"] == "morpho_00001_.png"
    assert transport.paths().count(f"/history/{PROMPT_ID}") == 3


def test_an_execution_error_is_permanent(transport: FakeTransport) -> None:
    transport.history = {
        PROMPT_ID: {
            "status": {
                "status_str": "error",
                "completed": False,
                "messages": [
                    ["execution_error", {"node_type": "KSampler", "exception_message": "OOM"}]
                ],
            },
            "outputs": {},
        }
    }
    with pytest.raises(PermanentError) as excinfo:
        client(transport).await_images(PROMPT_ID, "9")
    assert "OOM" in excinfo.value.message


def test_finishing_with_no_image_is_permanent(transport: FakeTransport) -> None:
    transport.history = {
        PROMPT_ID: {"status": {"status_str": "success", "completed": True}, "outputs": {}}
    }
    with pytest.raises(PermanentError, match="without an image"):
        client(transport).await_images(PROMPT_ID, "9")


def test_a_renamed_output_node_still_yields_the_image(transport: FakeTransport) -> None:
    transport.history = history_payload(node="17")
    images = client(transport).await_images(PROMPT_ID, "9")
    assert images[0]["filename"] == "morpho_00001_.png"


def test_a_stalled_render_is_transient(
    transport: FakeTransport, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv(comfy.TIMEOUT_ENV, "0.05")
    monkeypatch.setenv(comfy.POLL_INTERVAL_ENV, "0.01")
    transport.history = {PROMPT_ID: {"status": {"status_str": "running"}, "outputs": {}}}
    with pytest.raises(TransientError, match="did not finish"):
        client(transport).await_images(PROMPT_ID, "9")


# --- download ---------------------------------------------------------------


def test_download_builds_the_view_query(transport: FakeTransport) -> None:
    data = client(transport).download(
        {"filename": "morpho_00001_.png", "subfolder": "sub dir", "type": "output"}
    )
    assert data == transport.image
    path = transport.paths()[-1]
    assert path.startswith("/view?")
    assert "filename=morpho_00001_.png" in path
    assert "subfolder=sub+dir" in path
    assert "type=output" in path


def test_an_empty_download_is_transient(transport: FakeTransport) -> None:
    transport.view_response = HttpResponse(status=200, body=b"")
    with pytest.raises(TransientError, match="empty image"):
        client(transport).download({"filename": "x.png"})


def test_a_missing_render_is_permanent(transport: FakeTransport) -> None:
    transport.view_response = HttpResponse(status=404, body=b"not found")
    with pytest.raises(PermanentError):
        client(transport).download({"filename": "x.png"})


# --- env parsing ------------------------------------------------------------


def test_timeouts_come_from_env(monkeypatch: pytest.MonkeyPatch) -> None:
    assert comfy._timeout() == comfy.DEFAULT_TIMEOUT_S
    monkeypatch.setenv(comfy.TIMEOUT_ENV, "120")
    assert comfy._timeout() == 120.0
    monkeypatch.setenv(comfy.TIMEOUT_ENV, "soon")
    assert comfy._timeout() == comfy.DEFAULT_TIMEOUT_S
    monkeypatch.setenv(comfy.CONNECT_TIMEOUT_ENV, "0")
    assert comfy._connect_timeout() == comfy.DEFAULT_CONNECT_TIMEOUT_S
