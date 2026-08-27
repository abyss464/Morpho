"""Hash → file, and the things that are not hashes."""

from __future__ import annotations

import pytest

from morpho_clip.media import MediaLibrary, is_content_hash

HASH = "a" * 64
OTHER = "b" * 64


@pytest.fixture
def library(tmp_path):
    shard = tmp_path / HASH[:2]
    shard.mkdir()
    (shard / f"{HASH}.webp").write_bytes(b"not really a webp")
    return MediaLibrary(tmp_path)


def test_a_stored_picture_resolves(library):
    found = library.path_for(HASH)
    assert found is not None
    assert found.name == f"{HASH}.webp"


def test_a_hash_with_nothing_behind_it_is_none(library):
    assert library.path_for(OTHER) is None


def test_other_extensions_are_found_too(tmp_path):
    shard = tmp_path / HASH[:2]
    shard.mkdir()
    (shard / f"{HASH}.png").write_bytes(b"png")
    assert MediaLibrary(tmp_path).path_for(HASH).suffix == ".png"


def test_a_missing_root_is_not_an_error(tmp_path):
    assert MediaLibrary(tmp_path / "nowhere").path_for(HASH) is None


@pytest.mark.parametrize(
    "value",
    [
        "",
        "short",
        "A" * 64,  # uppercase is not the digest form we write
        "g" * 64,  # not hex
        "../" + "a" * 61,
        "a" * 63 + "/",
        "a" * 65,
    ],
)
def test_only_a_real_content_hash_is_accepted(value):
    """A hash is joined onto a root, so anything that is not one is a traversal
    waiting to happen. `path_for` refuses before it touches the filesystem."""
    assert not is_content_hash(value)
    assert MediaLibrary("/tmp").path_for(value) is None


def test_a_real_hash_is_accepted():
    assert is_content_hash(HASH)
    assert is_content_hash("0123456789abcdef" * 4)
