"""Versioned ComfyUI workflow templates.

The template file is plain ComfyUI API-format JSON under a `graph` key plus a
small binding map naming which node input each request parameter drives. Keeping
the graph literal (rather than sprinkling `%placeholders%` through it) means the
file can be pasted straight into ComfyUI to debug a bad render, and adding a node
never requires touching the substitution code.

Templates are versioned by filename. Changing sampler, steps or cfg changes the
pixels, so it changes the file: `sdxl_txt2img_v2.json`, never an edit in place.
"""

from __future__ import annotations

import json
import logging
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Final

from morpho_adapter_common import PermanentError

log = logging.getLogger("morpho.adapter.sdxl.workflow")

WORKFLOW_ENV: Final[str] = "MORPHO_SDXL_WORKFLOW"
CHECKPOINT_ENV: Final[str] = "MORPHO_SDXL_CHECKPOINT"

WORKFLOW_DIR: Final[Path] = Path(__file__).resolve().parent / "workflows"
DEFAULT_WORKFLOW: Final[str] = "sdxl_txt2img_v1.json"

REQUIRED_BINDINGS: Final[tuple[str, ...]] = (
    "checkpoint",
    "positive_prompt",
    "negative_prompt",
    "seed",
    "width",
    "height",
)


@dataclass(frozen=True)
class WorkflowTemplate:
    name: str
    template_version: int
    model: str
    output_node: str
    bindings: dict[str, tuple[str, str]]
    graph: dict[str, Any]

    def checkpoint(self) -> str:
        node, field = self.bindings["checkpoint"]
        return str(self.graph[node]["inputs"][field])

    def render(
        self,
        *,
        prompt: str,
        negative_prompt: str,
        seed: int,
        width: int,
        height: int,
        checkpoint: str | None = None,
    ) -> dict[str, Any]:
        """Return a fresh graph with the request's values bound in."""
        graph = json.loads(json.dumps(self.graph))
        values: dict[str, Any] = {
            "positive_prompt": prompt,
            "negative_prompt": negative_prompt,
            "seed": seed,
            "width": width,
            "height": height,
        }
        if checkpoint:
            values["checkpoint"] = checkpoint
        for key, value in values.items():
            node, field = self.bindings[key]
            graph[node]["inputs"][field] = value
        return graph


def workflow_path() -> Path:
    override = os.environ.get(WORKFLOW_ENV)
    if override:
        path = Path(override).expanduser()
        if not path.is_file():
            raise PermanentError(f"{WORKFLOW_ENV} points at a missing file: {path}")
        return path
    return WORKFLOW_DIR / DEFAULT_WORKFLOW


def load(path: Path | None = None) -> WorkflowTemplate:
    target = path or workflow_path()
    try:
        payload = json.loads(target.read_text(encoding="utf-8"))
    except OSError as exc:
        raise PermanentError(f"cannot read workflow template {target}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise PermanentError(f"workflow template {target} is not valid JSON: {exc}") from exc
    return _validate(target.name, payload)


def _validate(name: str, payload: Any) -> WorkflowTemplate:
    if not isinstance(payload, dict):
        raise PermanentError(f"workflow template {name} must be a JSON object")
    graph = payload.get("graph")
    if not isinstance(graph, dict) or not graph:
        raise PermanentError(f"workflow template {name} has no 'graph'")
    raw_bindings = payload.get("bindings")
    if not isinstance(raw_bindings, dict):
        raise PermanentError(f"workflow template {name} has no 'bindings'")

    bindings: dict[str, tuple[str, str]] = {}
    for key in REQUIRED_BINDINGS:
        binding = raw_bindings.get(key)
        if not isinstance(binding, list) or len(binding) != 2:
            raise PermanentError(f"workflow template {name} binding {key!r} must be [node, field]")
        node, field = str(binding[0]), str(binding[1])
        node_spec = graph.get(node)
        if not isinstance(node_spec, dict) or not isinstance(node_spec.get("inputs"), dict):
            raise PermanentError(
                f"workflow template {name} binding {key!r} names unknown node {node!r}"
            )
        if field not in node_spec["inputs"]:
            raise PermanentError(
                f"workflow template {name} binding {key!r} names unknown input {field!r} "
                f"on node {node!r}"
            )
        bindings[key] = (node, field)

    output_node = str(payload.get("output_node", ""))
    if output_node not in graph:
        raise PermanentError(f"workflow template {name} 'output_node' is not part of the graph")

    return WorkflowTemplate(
        name=name,
        template_version=int(payload.get("template_version", 1)),
        model=str(payload.get("model", "sdxl")),
        output_node=output_node,
        bindings=bindings,
        graph=graph,
    )


def effective_checkpoint(template: WorkflowTemplate) -> str:
    return os.environ.get(CHECKPOINT_ENV) or template.checkpoint()


def model_label(template: WorkflowTemplate) -> str:
    """What goes into the response's `model` field.

    The template carries a human label (`sdxl-base-1.0`). An operator who points
    the adapter at a different checkpoint gets that file's stem instead, so the
    recorded `source_ref` never claims a model that did not produce the image.
    """
    override = os.environ.get(CHECKPOINT_ENV)
    if override:
        return Path(override).stem
    return template.model
