"""Bucket selection and the pinned WebP encode."""

from __future__ import annotations

from pathlib import Path

import pytest
from morpho_adapter_common import PermanentError
from PIL import Image

from conftest import png_bytes
from morpho_sdxl import image

# --- native resolution buckets ----------------------------------------------


@pytest.mark.parametrize(
    ("requested", "expected"),
    [
        ((768, 576), (1152, 896)),  # 4:3, the release spec
        ((1024, 1024), (1024, 1024)),
        ((576, 768), (896, 1152)),
        ((1024, 576), (1344, 768)),  # 16:9
        ((640, 360), (1344, 768)),
    ],
)
def test_generation_picks_the_nearest_native_bucket(
    requested: tuple[int, int], expected: tuple[int, int]
) -> None:
    assert image.generation_size(*requested) == expected


def test_bucket_selection_is_stable() -> None:
    assert image.generation_size(768, 576) == image.generation_size(768, 576)


def test_a_request_larger_than_every_bucket_is_honoured() -> None:
    assert image.generation_size(2000, 1500) == (2048, 1536)


def test_buckets_can_be_disabled(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(image.NATIVE_BUCKETS_ENV, "0")
    assert image.generation_size(768, 576) == (768, 576)
    assert image.generation_size(770, 570) == (832, 576), "still rounded to a multiple of 64"


# --- encoding ---------------------------------------------------------------


def test_render_is_downscaled_to_the_requested_size(tmp_path: Path) -> None:
    out = tmp_path / "x.webp"
    image.to_webp(png_bytes(1152, 896), out, 768, 576)
    with Image.open(out) as written:
        assert written.format == "WEBP"
        assert written.size == (768, 576)
        assert written.mode == "RGB"


def test_a_render_already_at_size_is_not_resampled(tmp_path: Path) -> None:
    out = tmp_path / "x.webp"
    image.to_webp(png_bytes(768, 576), out, 768, 576)
    with Image.open(out) as written:
        assert written.size == (768, 576)


def test_encoding_is_byte_deterministic(tmp_path: Path) -> None:
    source = png_bytes(1152, 896)
    first, second = tmp_path / "a.webp", tmp_path / "b.webp"
    image.to_webp(source, first, 768, 576)
    image.to_webp(source, second, 768, 576)
    assert first.read_bytes() == second.read_bytes()


def test_no_metadata_is_carried_into_the_output(tmp_path: Path) -> None:
    out = tmp_path / "x.webp"
    image.to_webp(png_bytes(256, 256), out, 128, 128)
    with Image.open(out) as written:
        assert not written.info.get("icc_profile")
        assert not written.info.get("exif")


def test_quality_is_configurable(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    source = png_bytes(512, 512)
    default_out = tmp_path / "default.webp"
    image.to_webp(source, default_out, 512, 512)
    monkeypatch.setenv(image.QUALITY_ENV, "20")
    low_out = tmp_path / "low.webp"
    image.to_webp(source, low_out, 512, 512)
    assert low_out.stat().st_size < default_out.stat().st_size


def test_out_of_range_quality_falls_back(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(image.QUALITY_ENV, "0")
    assert image._quality() == image.DEFAULT_QUALITY
    monkeypatch.setenv(image.QUALITY_ENV, "101")
    assert image._quality() == image.DEFAULT_QUALITY
    monkeypatch.setenv(image.QUALITY_ENV, "high")
    assert image._quality() == image.DEFAULT_QUALITY


def test_rgba_renders_are_flattened(tmp_path: Path) -> None:
    import io

    buffer = io.BytesIO()
    Image.new("RGBA", (256, 256), (10, 20, 30, 128)).save(buffer, format="PNG")
    out = tmp_path / "x.webp"
    image.to_webp(buffer.getvalue(), out, 128, 128)
    with Image.open(out) as written:
        assert written.mode == "RGB"


def test_non_image_bytes_are_permanent(tmp_path: Path) -> None:
    with pytest.raises(PermanentError, match="not an image"):
        image.to_webp(b"<html>ComfyUI error page</html>", tmp_path / "x.webp", 64, 64)


def test_a_failed_encode_leaves_no_partial_file(tmp_path: Path) -> None:
    out = tmp_path / "x.webp"
    with pytest.raises(PermanentError):
        image.to_webp(b"junk", out, 64, 64)
    assert not out.exists()
    assert list(tmp_path.iterdir()) == []
