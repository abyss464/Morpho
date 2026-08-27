"""The model, and the seam that lets everything else be tested without one.

`Scorer` is the whole interface the service needs: an identity, and a way to
turn one text plus a list of image paths into cosines. `OpenClipScorer` is the
real implementation, and it imports torch lazily — so the routing, the request
validation and the hash resolution are all exercised by the test suite on a
machine with no accelerator and no model weights.

The embedding logic is `ops/clip_rematch.py`'s, unchanged: encode, L2-normalize,
take the dot product. That script's numbers are the ones the thresholds in
`morpho_reconcile::score` were read off, so reproducing them exactly is not a
detail — a different normalization would silently move every threshold in the
engine.
"""

from __future__ import annotations

import logging
import threading
from collections.abc import Sequence
from pathlib import Path
from typing import Final, Protocol

log = logging.getLogger("morpho.clip.scorer")

#: The algorithm version this service implements. It must match
#: `morpho_domain::version::CLIP_ALGO_VER`; the engine refuses scores from a
#: sidecar whose identity differs from the one it stores under, so a mismatch
#: fails loudly on the first request rather than quietly filing two models'
#: numbers in one column.
ALGO_VER: Final[str] = "clip/1"

DEFAULT_ARCH: Final[str] = "ViT-B-32"
DEFAULT_PRETRAINED: Final[str] = "laion2b_s34b_b79k"


class Scorer(Protocol):
    """What the service needs from a model."""

    @property
    def model(self) -> str:
        """`<arch>/<pretrained>`, the identity the engine checks."""

    def score(self, text: str, paths: Sequence[Path]) -> list[float]:
        """Cosine between `text` and each picture, in the order given."""


class OpenClipScorer:
    """open_clip on whatever accelerator this process can reach.

    The model is loaded once, on the first request rather than at construction,
    so `--help` and a misconfigured media root both fail in milliseconds instead
    of after a weight download. Loading is guarded by a lock because the stdlib
    threading server answers requests concurrently and two threads racing to
    build the same model would put two copies on the device.
    """

    def __init__(
        self,
        arch: str = DEFAULT_ARCH,
        pretrained: str = DEFAULT_PRETRAINED,
        device: str | None = None,
    ) -> None:
        self.arch = arch
        self.pretrained = pretrained
        self._requested_device = device
        self._lock = threading.Lock()
        self._loaded: object | None = None

    @property
    def model(self) -> str:
        return f"{self.arch}/{self.pretrained}"

    def score(self, text: str, paths: Sequence[Path]) -> list[float]:
        torch, model, preprocess, tokenizer, device = self._load()
        from PIL import Image

        with torch.no_grad():
            query = model.encode_text(tokenizer([text]).to(device))
            query = query / query.norm(dim=-1, keepdim=True)
            out: list[float] = []
            for path in paths:
                with Image.open(path) as handle:
                    tensor = preprocess(handle.convert("RGB")).unsqueeze(0).to(device)
                features = model.encode_image(tensor)
                features = features / features.norm(dim=-1, keepdim=True)
                out.append(float(query[0] @ features[0]))
        return out

    def _load(self):
        with self._lock:
            if self._loaded is None:
                self._loaded = self._build()
        return self._loaded

    def _build(self):
        import open_clip
        import torch

        device = self._requested_device or ("cuda" if torch.cuda.is_available() else "cpu")
        log.info("loading %s on %s", self.model, device)
        model, _, preprocess = open_clip.create_model_and_transforms(
            self.arch, pretrained=self.pretrained
        )
        tokenizer = open_clip.get_tokenizer(self.arch)
        model = model.to(device).eval()
        log.info("model ready")
        return torch, model, preprocess, tokenizer, device
