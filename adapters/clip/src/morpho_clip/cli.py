"""Entry point: `python -m morpho_clip` (or `clip-sidecar` where it is installed).

The usual invocation is under the venv that already holds a working accelerator
build of torch, which on this machine is ComfyUI's:

    ~/Code/vendor/ComfyUI/.venv/bin/python -m morpho_clip \\
        --media-root ~/Code/learning/Morpho/data/media --port 30013

That venv has `open_clip` and `torch` and knows nothing about this package, so
the module is run from the source tree via `PYTHONPATH` rather than installed
into it — see `docs/contracts/clip-service.md`.
"""

from __future__ import annotations

import argparse
import logging
import sys

from .media import MediaLibrary, default_root
from .scorer import DEFAULT_ARCH, DEFAULT_PRETRAINED, OpenClipScorer
from .service import ScoreService, serve

DEFAULT_PORT = 30013


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="morpho_clip",
        description="Score Morpho's image candidates against their words' sentences.",
    )
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument(
        "--media-root",
        default=None,
        help="content-addressed media library "
        f"(default: $MORPHO_CLIP_MEDIA_ROOT, else {default_root()})",
    )
    parser.add_argument("--arch", default=DEFAULT_ARCH)
    parser.add_argument("--pretrained", default=DEFAULT_PRETRAINED)
    parser.add_argument(
        "--device",
        default=None,
        help="torch device; defaults to cuda when one is visible, else cpu",
    )
    parser.add_argument("--quiet", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    logging.basicConfig(
        level=logging.WARNING if args.quiet else logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s %(message)s",
        stream=sys.stderr,
    )
    library = MediaLibrary(args.media_root or default_root())
    if not library.root.is_dir():
        # Better to say so now than to answer every request with a full
        # `missing` list and let somebody wonder why every word is unscored.
        logging.getLogger("morpho.clip").warning(
            "media root %s does not exist; every picture will read as missing", library.root
        )
    service = ScoreService(
        OpenClipScorer(arch=args.arch, pretrained=args.pretrained, device=args.device),
        library,
    )
    serve(service, args.host, args.port)
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
