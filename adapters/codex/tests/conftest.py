"""A fake generator, so nothing here spends anybody's quota.

`MORPHO_CODEX_BIN` points the runner at an executable of the test's choosing,
which is the same seam an operator uses to point it at a real one. The stub
scripts written here behave like the CLI does in each of the cases that matter:
it draws, it refuses silently, it produces a blank canvas, it fails.
"""

from __future__ import annotations

import os
import stat
import sys
from pathlib import Path

import pytest
from PIL import Image


def write_image(path: Path, size: tuple[int, int] = (1024, 1024), noisy: bool = True) -> None:
    """A picture, or — with `noisy=False` — a flat white canvas."""
    if not noisy:
        Image.new("RGB", size, (255, 255, 255)).save(path)
        return
    image = Image.new("RGB", size)
    pixels = image.load()
    for x in range(size[0]):
        for y in range(size[1]):
            pixels[x, y] = ((x * 7) % 256, (y * 11) % 256, ((x + y) * 3) % 256)
    image.save(path)


@pytest.fixture
def fake_codex(tmp_path, monkeypatch):
    """Install a stub generator and return a way to reconfigure what it does."""
    script = tmp_path / "fake-codex"

    def configure(body: str) -> None:
        script.write_text(f"#!{sys.executable}\n{body}\n")
        script.chmod(script.stat().st_mode | stat.S_IEXEC)

    # Reads the prompt off stdin, finds the path it names, draws there.
    configure(
        "import re, sys\n"
        "from pathlib import Path\n"
        f"sys.path.insert(0, {str(Path(__file__).parent)!r})\n"
        "from conftest import write_image\n"
        "prompt = sys.stdin.read()\n"
        "target = re.search(r'exact path:\\n(\\S+)', prompt).group(1)\n"
        "write_image(Path(target))\n"
    )
    monkeypatch.setenv("MORPHO_CODEX_BIN", str(script))
    monkeypatch.setenv("MORPHO_CODEX_ARGS", "exec")
    monkeypatch.setenv("MORPHO_CODEX_MODEL", "stub-generator")
    return configure


@pytest.fixture
def staging(tmp_path):
    """The directory morphod would have created, and the path it owns."""
    directory = tmp_path / "staging"
    directory.mkdir()
    return directory / "image.webp"


@pytest.fixture(autouse=True)
def _no_inherited_configuration(monkeypatch):
    """A developer's own `MORPHO_CODEX_*` must not decide what these assert."""
    for name in ("MORPHO_CODEX_BLANK_STDDEV", "MORPHO_CODEX_WEBP_QUALITY",
                 "MORPHO_CODEX_TIMEOUT_S"):
        monkeypatch.delenv(name, raising=False)
    os.environ.pop("MORPHO_CODEX_BIN", None)
