"""Adapter error taxonomy.

Mirrors morphod's `AdapterError` enum (README part 4) and the wire taxonomy in
`docs/contracts/adapter-protocol.md`:

- ``permanent``    -> morphod records completion/failure and never retries.
- ``transient``    -> morphod retries with exponential backoff.
- ``rate_limited`` -> morphod parks the whole lane until ``retry_after_ms``.
"""

from __future__ import annotations

from typing import Final, Literal

ErrorKind = Literal["permanent", "transient", "rate_limited"]

PERMANENT: Final[ErrorKind] = "permanent"
TRANSIENT: Final[ErrorKind] = "transient"
RATE_LIMITED: Final[ErrorKind] = "rate_limited"

ERROR_KINDS: Final[frozenset[str]] = frozenset({PERMANENT, TRANSIENT, RATE_LIMITED})

#: Used when a rate-limit signal carries no explicit cooldown.
DEFAULT_RATE_LIMIT_MS: Final[int] = 60_000


class AdapterError(Exception):
    """Base class for every error an adapter reports through the envelope."""

    kind: ErrorKind = TRANSIENT

    def __init__(self, message: str, *, retry_after_ms: int = 0) -> None:
        super().__init__(message)
        self.message = message
        self.retry_after_ms = max(0, int(retry_after_ms))

    def to_payload(self) -> dict[str, object]:
        return {
            "kind": self.kind,
            "message": self.message,
            "retry_after_ms": self.retry_after_ms,
        }


class PermanentError(AdapterError):
    """Legitimately unfulfillable request: bad params, missing tooling, 404."""

    kind: ErrorKind = PERMANENT


class TransientError(AdapterError):
    """Network hiccup, 5xx, timeout: worth retrying later."""

    kind: ErrorKind = TRANSIENT


class RateLimitedError(AdapterError):
    """Upstream asked us to slow down; parks the morphod lane."""

    kind: ErrorKind = RATE_LIMITED

    def __init__(self, message: str, *, retry_after_ms: int = DEFAULT_RATE_LIMIT_MS) -> None:
        super().__init__(message, retry_after_ms=max(1, int(retry_after_ms)))


class ProtocolError(Exception):
    """The request itself was unreadable — nothing sensible can go on stdout."""
