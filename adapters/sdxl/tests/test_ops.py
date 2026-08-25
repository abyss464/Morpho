"""`sdxl.generate` end to end against a scripted ComfyUI."""

from __future__ import annotations

import urllib.error
from pathlib import Path

import pytest
from morpho_adapter_common import PermanentError, dispatch
from PIL import Image

from conftest import FakeTransport
from morpho_sdxl import comfy
from morpho_sdxl.ops import OPS, generate


def _params(tmp_path: Path, **overrides: object) -> dict[str, object]:
    params: dict[str, object] = {
        "prompt": "a scene depicting the concept of 'abandon': an empty swing set at dusk",
        "negative_prompt": "text, watermark, logo",
        "seed": 42,
        "width": 768,
        "height": 576,
        "out_path": str(tmp_path / "x.webp"),
    }
    params.update(overrides)
    return params


def test_contract_example_shape(tmp_path: Path, transport: FakeTransport) -> None:
    result = generate(_params(tmp_path))
    assert result == {"model": "sdxl-base-1.0", "seed": 42}
    with Image.open(tmp_path / "x.webp") as written:
        assert written.format == "WEBP"
        assert written.size == (768, 576)


def test_response_envelope(tmp_path: Path, transport: FakeTransport) -> None:
    response = dispatch(OPS, "sdxl.generate", _params(tmp_path))
    assert response["ok"] is True
    assert response["result"]["seed"] == 42


def test_the_backend_is_probed_before_anything_is_submitted(
    tmp_path: Path, transport: FakeTransport
) -> None:
    generate(_params(tmp_path))
    paths = transport.paths()
    assert paths[0] == "/system_stats"
    assert paths[1] == "/prompt"
    assert any(path.startswith("/view") for path in paths)


def test_request_values_reach_the_graph(tmp_path: Path, transport: FakeTransport) -> None:
    generate(_params(tmp_path, prompt="serene", negative_prompt="blurry", seed=7))
    graph = transport.submitted_graph()
    assert graph["6"]["inputs"]["text"] == "serene"
    assert graph["7"]["inputs"]["text"] == "blurry"
    assert graph["3"]["inputs"]["seed"] == 7


def test_rendering_happens_at_a_native_resolution(tmp_path: Path, transport: FakeTransport) -> None:
    generate(_params(tmp_path))
    latent = transport.submitted_graph()["5"]["inputs"]
    assert (latent["width"], latent["height"]) == (1152, 896)


def test_the_checkpoint_override_reaches_the_graph_and_the_result(
    tmp_path: Path, transport: FakeTransport, monkeypatch: pytest.MonkeyPatch
) -> None:
    from morpho_sdxl import workflow

    monkeypatch.setenv(workflow.CHECKPOINT_ENV, "juggernautXL_v9.safetensors")
    result = generate(_params(tmp_path))
    assert transport.submitted_graph()["4"]["inputs"]["ckpt_name"] == "juggernautXL_v9.safetensors"
    assert result["model"] == "juggernautXL_v9"


def test_the_same_request_produces_the_same_bytes(tmp_path: Path, transport: FakeTransport) -> None:
    first, second = tmp_path / "a.webp", tmp_path / "b.webp"
    generate(_params(tmp_path, out_path=str(first)))
    generate(_params(tmp_path, out_path=str(second)))
    assert first.read_bytes() == second.read_bytes()


# --- the contract's unconfigured-backend path --------------------------------


def test_an_unreachable_backend_is_permanent(tmp_path: Path, transport: FakeTransport) -> None:
    transport.raise_on = {
        "/system_stats": urllib.error.URLError(ConnectionRefusedError(111, "Connection refused"))
    }
    with pytest.raises(PermanentError) as excinfo:
        generate(_params(tmp_path))
    assert excinfo.value.kind == "permanent"
    assert excinfo.value.message.startswith(comfy.NOT_CONFIGURED_MESSAGE)
    assert transport.paths() == ["/system_stats"], "nothing is submitted to a dead backend"
    assert not (tmp_path / "x.webp").exists()


def test_no_partial_file_when_the_render_fails(tmp_path: Path, transport: FakeTransport) -> None:
    from morpho_sdxl.comfy import HttpResponse

    transport.view_response = HttpResponse(status=200, body=b"not a png")
    out = tmp_path / "x.webp"
    with pytest.raises(PermanentError):
        generate(_params(tmp_path, out_path=str(out)))
    assert not out.exists()
    assert list(tmp_path.iterdir()) == []


# --- params -----------------------------------------------------------------


def test_defaults_match_the_release_spec(tmp_path: Path, transport: FakeTransport) -> None:
    generate({"prompt": "abandon", "out_path": str(tmp_path / "x.webp")})
    with Image.open(tmp_path / "x.webp") as written:
        assert written.size == (768, 576)
    graph = transport.submitted_graph()
    assert graph["3"]["inputs"]["seed"] == 0
    assert graph["7"]["inputs"]["text"] == "text, watermark, logo"


def test_an_empty_negative_prompt_is_allowed(tmp_path: Path, transport: FakeTransport) -> None:
    generate(_params(tmp_path, negative_prompt=""))
    assert transport.submitted_graph()["7"]["inputs"]["text"] == ""


@pytest.mark.parametrize(
    "overrides",
    [
        {"prompt": ""},
        {"prompt": 42},
        {"seed": -1},
        {"seed": 2**64},
        {"seed": "42"},
        {"width": 0},
        {"width": 4096},
        {"height": 32},
        {"out_path": "relative.webp"},
        {"prompt": "x" * 2001},
    ],
)
def test_bad_params_are_permanent(
    tmp_path: Path, transport: FakeTransport, overrides: dict[str, object]
) -> None:
    with pytest.raises(PermanentError):
        generate(_params(tmp_path, **overrides))


def test_missing_required_params_are_permanent(tmp_path: Path, transport: FakeTransport) -> None:
    with pytest.raises(PermanentError):
        generate({})
    with pytest.raises(PermanentError):
        generate({"prompt": "abandon"})


def test_params_are_validated_before_the_backend_is_touched(
    tmp_path: Path, transport: FakeTransport
) -> None:
    with pytest.raises(PermanentError):
        generate(_params(tmp_path, prompt=""))
    assert transport.calls == []
