"""Atomic writes to the engine-owned output path.

morphod hands the adapter a temp path, then hashes and renames the file into the
content-addressed store. If an adapter dies mid-write, morphod must never see a
truncated file at that path — so everything lands on a sibling `.part` file and
is renamed into place only once it is complete and fsynced.
"""

from __future__ import annotations

import contextlib
import os
from collections.abc import Iterator
from pathlib import Path


@contextlib.contextmanager
def staged_output(out_path: str | os.PathLike[str], suffix: str = ".part") -> Iterator[Path]:
    """Yield a staging path; rename it onto `out_path` when the block succeeds."""
    final = Path(out_path)
    staging = final.with_name(final.name + suffix)
    _unlink_quietly(staging)
    try:
        yield staging
        if not staging.exists():
            raise FileNotFoundError(f"nothing was written to {staging}")
        _fsync(staging)
        os.replace(staging, final)
    finally:
        _unlink_quietly(staging)


def write_bytes_atomic(out_path: str | os.PathLike[str], data: bytes) -> None:
    with staged_output(out_path) as staging:
        staging.write_bytes(data)


def _fsync(path: Path) -> None:
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def _unlink_quietly(path: Path) -> None:
    with contextlib.suppress(FileNotFoundError, IsADirectoryError, PermissionError):
        path.unlink()
