"""The shipped template is valid, and a broken one fails permanently."""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from morpho_adapter_common import PermanentError

from morpho_sdxl import workflow


def test_the_shipped_template_loads() -> None:
    template = workflow.load()
    assert template.name == workflow.DEFAULT_WORKFLOW
    assert template.template_version == 1
    assert template.model == "sdxl-base-1.0"
    assert set(template.bindings) == set(workflow.REQUIRED_BINDINGS)


def test_the_shipped_template_is_a_plain_comfyui_graph() -> None:
    template = workflow.load()
    for node_id, node in template.graph.items():
        assert isinstance(node_id, str)
        assert "class_type" in node, f"node {node_id} is missing class_type"
        assert isinstance(node["inputs"], dict)
    classes = {node["class_type"] for node in template.graph.values()}
    assert {"CheckpointLoaderSimple", "KSampler", "CLIPTextEncode", "VAEDecode"} <= classes


def test_the_template_ships_inside_the_package() -> None:
    assert (workflow.WORKFLOW_DIR / workflow.DEFAULT_WORKFLOW).is_file()


def test_render_binds_every_request_value() -> None:
    template = workflow.load()
    graph = template.render(
        prompt="a scene depicting the concept of 'abandon'",
        negative_prompt="text, watermark, logo",
        seed=42,
        width=1152,
        height=896,
        checkpoint="custom_sdxl.safetensors",
    )
    assert graph["6"]["inputs"]["text"] == "a scene depicting the concept of 'abandon'"
    assert graph["7"]["inputs"]["text"] == "text, watermark, logo"
    assert graph["3"]["inputs"]["seed"] == 42
    assert graph["5"]["inputs"]["width"] == 1152
    assert graph["5"]["inputs"]["height"] == 896
    assert graph["4"]["inputs"]["ckpt_name"] == "custom_sdxl.safetensors"


def test_render_leaves_the_template_untouched() -> None:
    template = workflow.load()
    template.render(prompt="one", negative_prompt="", seed=1, width=64, height=64)
    assert template.graph["6"]["inputs"]["text"] == ""
    assert template.graph["3"]["inputs"]["seed"] == 0


def test_render_is_reproducible() -> None:
    template = workflow.load()
    kwargs = {
        "prompt": "abandon",
        "negative_prompt": "text",
        "seed": 7,
        "width": 1152,
        "height": 896,
    }
    assert json.dumps(template.render(**kwargs), sort_keys=True) == json.dumps(
        template.render(**kwargs), sort_keys=True
    )


def test_sampler_settings_are_pinned() -> None:
    sampler = workflow.load().graph["3"]["inputs"]
    assert sampler["steps"] == 30
    assert sampler["cfg"] == 7.0
    assert sampler["sampler_name"] == "dpmpp_2m"
    assert sampler["scheduler"] == "karras"
    assert sampler["denoise"] == 1.0


def test_checkpoint_and_model_label(monkeypatch: pytest.MonkeyPatch) -> None:
    template = workflow.load()
    assert workflow.effective_checkpoint(template) == "sd_xl_base_1.0.safetensors"
    assert workflow.model_label(template) == "sdxl-base-1.0"
    monkeypatch.setenv(workflow.CHECKPOINT_ENV, "juggernautXL_v9.safetensors")
    assert workflow.effective_checkpoint(template) == "juggernautXL_v9.safetensors"
    assert workflow.model_label(template) == "juggernautXL_v9"


def test_env_override_selects_another_template(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    source = json.loads((workflow.WORKFLOW_DIR / workflow.DEFAULT_WORKFLOW).read_text())
    source["model"] = "custom-model"
    custom = tmp_path / "custom_v2.json"
    custom.write_text(json.dumps(source), encoding="utf-8")
    monkeypatch.setenv(workflow.WORKFLOW_ENV, str(custom))
    template = workflow.load()
    assert template.name == "custom_v2.json"
    assert template.model == "custom-model"


def test_missing_override_is_permanent(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(workflow.WORKFLOW_ENV, "/nonexistent/workflow.json")
    with pytest.raises(PermanentError, match="missing file"):
        workflow.load()


def _write(tmp_path: Path, payload: object) -> Path:
    path = tmp_path / "broken.json"
    path.write_text(json.dumps(payload), encoding="utf-8")
    return path


def test_invalid_json_is_permanent(tmp_path: Path) -> None:
    path = tmp_path / "broken.json"
    path.write_text("{not json", encoding="utf-8")
    with pytest.raises(PermanentError, match="not valid JSON"):
        workflow.load(path)


@pytest.mark.parametrize(
    ("payload", "match"),
    [
        ([], "must be a JSON object"),
        ({"bindings": {}}, "no 'graph'"),
        ({"graph": {"1": {"inputs": {}}}}, "no 'bindings'"),
        (
            {"graph": {"1": {"inputs": {"text": ""}}}, "bindings": {"checkpoint": "1"}},
            "must be .node, field.",
        ),
        (
            {
                "graph": {"1": {"inputs": {"ckpt_name": ""}}},
                "bindings": {
                    "checkpoint": ["99", "ckpt_name"],
                    "positive_prompt": ["1", "ckpt_name"],
                    "negative_prompt": ["1", "ckpt_name"],
                    "seed": ["1", "ckpt_name"],
                    "width": ["1", "ckpt_name"],
                    "height": ["1", "ckpt_name"],
                },
            },
            "unknown node",
        ),
        (
            {
                "graph": {"1": {"inputs": {"ckpt_name": ""}}},
                "bindings": {
                    "checkpoint": ["1", "nope"],
                    "positive_prompt": ["1", "ckpt_name"],
                    "negative_prompt": ["1", "ckpt_name"],
                    "seed": ["1", "ckpt_name"],
                    "width": ["1", "ckpt_name"],
                    "height": ["1", "ckpt_name"],
                },
            },
            "unknown input",
        ),
        (
            {
                "graph": {"1": {"inputs": {"ckpt_name": ""}}},
                "bindings": {key: ["1", "ckpt_name"] for key in workflow.REQUIRED_BINDINGS},
                "output_node": "42",
            },
            "output_node",
        ),
    ],
)
def test_malformed_templates_are_permanent(tmp_path: Path, payload: object, match: str) -> None:
    with pytest.raises(PermanentError, match=match):
        workflow.load(_write(tmp_path, payload))
