"""Entry point: `uv run --project adapters/tts tts-adapter`."""

from __future__ import annotations

import sys

from morpho_adapter_common import run

from .ops import OPS

ADAPTER_NAME = "tts"


def main() -> int:
    code = run(ADAPTER_NAME, OPS)
    sys.exit(code)


if __name__ == "__main__":  # pragma: no cover
    main()
