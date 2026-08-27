"""Resolving a content hash to the picture it names.

The media library is content addressed — `data/media/{hash[:2]}/{hash}.webp` —
so the engine sends hashes and this side reads bytes. Nothing about a candidate,
a word or a database crosses the wire.

The root is configured separately from the engine's on purpose: morphod sees the
library through a bind mount at `/app/data/media` and this process sees it at
whatever path the host keeps it under. A hash means the same file either way; a
path would not.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Final

#: Extensions the library is known to hold. `.webp` is what the engine's encoder
#: writes for every picture it stores; the rest are here so a hand-placed file or
#: a future encoder change does not read as a missing picture.
EXTENSIONS: Final[tuple[str, ...]] = (".webp", ".png", ".jpg", ".jpeg")

MEDIA_ROOT_ENV: Final[str] = "MORPHO_CLIP_MEDIA_ROOT"

#: A blake3 hex digest. Anything else is a caller bug, and letting it through
#: would turn a hash into a path traversal.
HASH_LENGTH: Final[int] = 64
_HEX: Final[frozenset[str]] = frozenset("0123456789abcdef")


def is_content_hash(value: str) -> bool:
    """Is this a plausible content address, rather than a path in disguise?

    Checked before anything is joined onto the root: `../../etc/passwd` is a
    perfectly good string and a very bad file name.
    """
    return (
        len(value) == HASH_LENGTH
        and not set(value) - _HEX
    )


def default_root() -> Path:
    """Where the library is, unless the caller says otherwise."""
    configured = os.environ.get(MEDIA_ROOT_ENV, "").strip()
    return Path(configured) if configured else Path("data/media")


class MediaLibrary:
    """A read-only view of the content-addressed media store."""

    def __init__(self, root: str | os.PathLike[str]) -> None:
        self.root = Path(root)

    def path_for(self, file_hash: str) -> Path | None:
        """The file this hash names, or `None` when nothing is there.

        A hash that is not a hash returns `None` rather than raising: the caller
        reports it in `missing`, which is the honest answer — this service
        cannot see a picture by that name — and one bad entry must not cost a
        word the scores of its other candidates.
        """
        if not is_content_hash(file_hash):
            return None
        shard = self.root / file_hash[:2]
        for extension in EXTENSIONS:
            candidate = shard / f"{file_hash}{extension}"
            if candidate.is_file():
                return candidate
        return None
