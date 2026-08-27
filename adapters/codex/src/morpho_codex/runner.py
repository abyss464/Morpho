"""Driving the external generator.

`ops/genimg_cron.sh` invoked it as `codex exec --dangerously-bypass-approvals-and-sandbox
-C <dir> -` with the prompt on stdin, and that is what this reproduces — the one
difference being that the working directory and the output path are morphod's
staging directory rather than a scratchpad the script owned.

Everything about the invocation is overridable from the environment, because the
binary is somebody else's and its flags are not ours to depend on. What is not
overridable is the taxonomy: a missing binary is `permanent` (no retry installs
it), a non-zero exit is `permanent` (the same prompt produces the same refusal),
a timeout is `transient` (the queue may simply be busy).
"""

from __future__ import annotations

import logging
import os
import shlex
import subprocess
from pathlib import Path
from typing import Final

from morpho_adapter_common import PermanentError, TransientError, resolve_binary

log = logging.getLogger("morpho.adapter.codex.runner")

#: Name of the generator binary, and the override the engine reads too — both
#: sides check the same variable, so a machine where the CLI is absent has the
#: source disabled rather than dead-lettering every word (README part 4,
#: "disabled == waived").
BIN_ENV: Final[str] = "MORPHO_CODEX_BIN"
DEFAULT_BIN: Final[str] = "codex"

#: Arguments between the binary and the prompt. Overridable as a shell-quoted
#: string, because these are the upstream CLI's flags and it is free to change
#: them without asking us.
ARGS_ENV: Final[str] = "MORPHO_CODEX_ARGS"
DEFAULT_ARGS: Final[tuple[str, ...]] = (
    "exec",
    "--dangerously-bypass-approvals-and-sandbox",
)

#: Below morphod's own 900 s job timeout, so the adapter reports a timeout the
#: engine can classify rather than being killed mid-write.
TIMEOUT_ENV: Final[str] = "MORPHO_CODEX_TIMEOUT_S"
DEFAULT_TIMEOUT_S: Final[float] = 840.0

BACKEND_MISSING: Final[str] = "codex backend not configured"


def available() -> bool:
    """Is the generator reachable at all?"""
    try:
        resolve_binary(DEFAULT_BIN, env_var=BIN_ENV)
    except PermanentError:
        return False
    return True


def generate(prompt: str, workdir: Path) -> str:
    """Run one generation. Returns the model label to record on the candidate.

    The generator writes the file itself, at the path the prompt names; this
    only drives it and reports what happened. Whether anything usable landed is
    the caller's question, because "the CLI exited 0 and wrote nothing" is a
    perfectly ordinary content-policy refusal.
    """
    try:
        binary = resolve_binary(
            DEFAULT_BIN,
            env_var=BIN_ENV,
            hint=f"set {BIN_ENV} or put it on PATH",
        )
    except PermanentError as exc:
        # Mirrors the sdxl adapter's "sdxl backend not configured": a word whose
        # generator is absent surfaces in dead letters with a message an
        # operator can act on, in milliseconds rather than after a timeout.
        raise PermanentError(BACKEND_MISSING) from exc

    argv = [binary, *_args(), "-"]
    log.info("generating via %s", " ".join(argv))
    try:
        proc = subprocess.run(
            argv,
            input=prompt.encode(),
            capture_output=True,
            cwd=workdir,
            timeout=_timeout(),
            check=False,
        )
    except FileNotFoundError as exc:
        raise PermanentError(BACKEND_MISSING) from exc
    except subprocess.TimeoutExpired as exc:
        raise TransientError(f"codex generation timed out after {_timeout():g}s") from exc
    except OSError as exc:
        raise TransientError(f"codex could not be started: {exc}") from exc

    if proc.returncode != 0:
        detail = proc.stderr.decode("utf-8", "replace").strip()
        tail = " | ".join(detail.splitlines()[-4:])
        raise PermanentError(
            f"codex exited {proc.returncode}" + (f": {tail}" if tail else "")
        )
    return os.environ.get("MORPHO_CODEX_MODEL", "codex").strip() or "codex"


def _args() -> list[str]:
    raw = os.environ.get(ARGS_ENV, "").strip()
    return shlex.split(raw) if raw else list(DEFAULT_ARGS)


def _timeout() -> float:
    raw = os.environ.get(TIMEOUT_ENV, "").strip()
    if not raw:
        return DEFAULT_TIMEOUT_S
    try:
        value = float(raw)
    except ValueError:
        log.warning("ignoring non-numeric %s=%r", TIMEOUT_ENV, raw)
        return DEFAULT_TIMEOUT_S
    return value if value > 0 else DEFAULT_TIMEOUT_S
