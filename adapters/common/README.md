# morpho-adapter-common

Shared protocol layer for the Morpho Python adapters. Implements the envelope
described in `docs/contracts/adapter-protocol.md`:

- read exactly one JSON request from stdin,
- write exactly one JSON response to stdout,
- keep every log line on stderr,
- exit 0 whenever the protocol was honored (success *or* error response),
  non-zero only on protocol-level failures.

See `adapters/README.md` for the full operator guide.
