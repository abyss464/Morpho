"""External binary discovery and invocation.

Adapters shell out to pinned tools (ffmpeg today). A missing binary is a
`permanent` failure: no amount of retrying installs it, and the operator needs
to see the word in dead letters with an actionable message.
"""

from __future__ import annotations

import logging
import os
import shutil
import subprocess
from collections.abc import Sequence

from .errors import PermanentError, TransientError

log = logging.getLogger("morpho.adapter.proc")


def resolve_binary(name: str, *, env_var: str | None = None, hint: str = "") -> str:
    """Locate an executable, honoring an explicit env override first."""
    override = os.environ.get(env_var) if env_var else None
    if override:
        if os.path.isfile(override) and os.access(override, os.X_OK):
            return override
        found = shutil.which(override)
        if found:
            return found
        raise PermanentError(
            f"{name} not usable at {env_var}={override!r}" + (f" ({hint})" if hint else "")
        )
    found = shutil.which(name)
    if not found:
        suffix = f" ({hint})" if hint else ""
        raise PermanentError(f"{name} not found on PATH; install it to run this adapter{suffix}")
    return found


def run_binary(
    argv: Sequence[str],
    *,
    timeout_s: float,
    what: str,
) -> subprocess.CompletedProcess[bytes]:
    """Run a subprocess with a hard timeout, mapping failures onto the taxonomy.

    A non-zero exit is `permanent` (bad input or bad invocation — both stable
    across retries); a timeout is `transient` (the box may simply be loaded).
    """
    log.debug("exec %s", " ".join(argv))
    try:
        proc = subprocess.run(
            list(argv),
            stdin=subprocess.DEVNULL,
            capture_output=True,
            timeout=timeout_s,
            check=False,
        )
    except FileNotFoundError as exc:
        raise PermanentError(f"{what} binary disappeared: {exc}") from exc
    except subprocess.TimeoutExpired as exc:
        raise TransientError(f"{what} timed out after {timeout_s:g}s") from exc
    except OSError as exc:
        raise TransientError(f"{what} could not be started: {exc}") from exc
    if proc.returncode != 0:
        detail = proc.stderr.decode("utf-8", "replace").strip()
        tail = detail.splitlines()[-4:] if detail else []
        raise PermanentError(
            f"{what} exited {proc.returncode}" + (f": {' | '.join(tail)}" if tail else "")
        )
    return proc
