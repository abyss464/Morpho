# adapters/common

6 specs.


============================================================
adapters/common/src/morpho_adapter_common/
============================================================


--- __init__.py ---

# Adapter Common Protocol Layer

Package entry point, aggregating the public interfaces of all exported submodules.

from `envelope` export:EXIT_PROTOCOL_FAILURE, Handler, configure_logging, dispatch, failure, parse_request, run, success, write_response

from `errors` export:DEFAULT_RATE_LIMIT_MS, ERROR_KINDS, PERMANENT, RATE_LIMITED, TRANSIENT, AdapterError, ErrorKind, PermanentError, ProtocolError, RateLimitedError, TransientError

from `outfile` export:staged_output, write_bytes_atomic

from `params` export:require_choice, require_int, require_out_path, require_str, require_str_list

from `proc` export:resolve_binary, run_binary


--- envelope.py ---

# JSON Envelope Protocol

All Morpho adapters share the same stdin/stdout JSON envelope. Request format: `{"op": "<name>", "params": {...}}`, response: `{"ok": true, "result": {...}}` or `{"ok": false, "error": {...}}`. stdout only carries response JSON, all logs go to stderr. Normal protocol termination (including error responses) exits with code 0; only when the request itself cannot be parsed is a non-zero exit code returned.

## Handler

Operation handler signature: receives a parameter mapping and returns a result mapping serializable to JSON.

## EXIT_PROTOCOL_FAILURE

Protocol-level failure exit code (value 2). Used when stdin is unreadable, JSON is malformed, or the envelope lacks a string-type `op` field. morphod treats any non-zero exit code as an adapter crash.

## configure_logging(adapter)

Configures logging to output only to stderr. The log level can be adjusted via the environment variable `MORPHO_ADAPTER_LOG_LEVEL`, defaulting to INFO.

## success(result) -> dictionary

Takes a result mapping and wraps it into a `{"ok": true, "result": ...}` envelope.

## failure(error) -> dictionary

Takes an AdapterError and wraps it into a `{"ok": false, "error": ...}` envelope.

## parse_request(raw) -> (op, params)

Parses a single request envelope's raw JSON string and returns the operation name and parameter mapping. Throws ProtocolError on parse failure.

## dispatch(ops, op, params) -> dictionary

Looks up the table by operation name to call the corresponding Handler, converting the execution result or exception into a response envelope. Unknown operations return a permanent error; AdapterError thrown by Handler is returned according to its classification; unexpected exceptions are uniformly treated as transient, with the traceback written to stderr.

## write_response(response, stream)

Writes a compact JSON with sorted keys to the stream, then flushes.

## run(adapter, ops, *, stdin, stdout) -> exit code

Complete adapter lifecycle: configure logging -> read stdin -> parse request -> dispatch execution -> write response. During Handler execution, stdout is redirected to stderr to prevent third-party database output from polluting the response JSON. Returns the process exit code.


--- errors.py ---

# Adapter Error Classification

Error classification aligned with morphod's retry strategy. Three errors determine morphod's subsequent behavior.

## ErrorKind

Error type identifier; values are one of "permanent", "transient", or "rate_limited".

## constant

- PERMANENT — Permanent error identifier
- TRANSIENT — Transient error identifier
- RATE_LIMITED — Rate-limited error identifier
- ERROR_KINDS — Frozen set of the above three identifiers
- DEFAULT_RATE_LIMIT_MS — Default value (60 seconds) when the rate-limit signal does not specify a cool-down duration

## AdapterError(message, *, retry_after_ms=0)

Base class for all errors reported through the Envelope. Carries the error message and the number of milliseconds to wait before retrying.

### to_payload() -> dictionary

Serializes to the format of the `error` field in the envelope, containing `kind`, `message`, and `retry_after_ms`.

## PermanentError

The request itself cannot be fulfilled (parameter error, missing tool, 404, etc.); morphod will not retry.

## TransientError

Temporary failures such as network jitter, 5xx, timeouts, etc.; morphod will retry with exponential backoff.

## RateLimitedError(message, *, retry_after_ms=60000)

The upstream requires throttling; morphod pauses the entire items channel until the cool-down period ends. `retry_after_ms` has a minimum of 1.

## ProtocolError

The request itself is unreadable and cannot produce any meaningful response on stdout.


--- outfile.py ---

# Atomic Write

Guarantee that the output file is either fully written to disk or not stored at all. morphod first gives the adapter a temporary path; after the adapter finishes writing, morphod hashes and renames it to content-addressed storage. The write first goes to a `.part` file in the same directory, then after fsync is atomically renamed, preventing truncated files from being left behind if the adapter crashes midway.

## staged_output(out_path, suffix=".part") -> Context Manager

On entry, it produces a temporary path, and within the block, data is written to that path. After the block exits normally, it automatically fsyncs and atomically replaces out_path; if the block exits abnormally or the temporary file does not exist, the temporary file is cleaned up and the exception is raised.

## write_bytes_atomic(out_path, data)

Takes byte data and atomically writes to out_path. Internally uses staged_output.


--- params.py ---

# Request Parameter Validation

A set of utility functions for extracting and validating values of each type from the request parameter map. All validation failures raise PermanentError—the adapter can never handle what morphod sent, so retrying is meaningless.

## require_str(params, name, *, default, allow_empty=False) -> string

Extract a string parameter. Raises an error if it is missing with no default value, the type is wrong, or it is blank (unless allow_empty is true).

## require_int(params, name, *, default, minimum, maximum) -> integer

Extract an integer parameter. Lower and upper bounds can be specified; boolean values are not considered integers.

## require_str_list(params, name, *, allow_empty=False) -> stringlist

Extract a string-array parameter. Check the type and non-emptiness of each item.

## require_choice(params, name, allowed, *, default) -> string

Extract a string parameter and verify its value is in the allowlist.

## require_out_path(params, name="out_path") -> string

Extract an output path parameter. It must be an absolute path and its parent directory must exist.


--- proc.py ---

# External binary call

Locate and execute external executable files (e.g., ffmpeg). Failure to find a binary is a permanent error—retrying will not conjure it up out of thin air; ops needs to see clear information in the dead letter.

## resolve_binary(name, *, env_var, hint) -> pathstring

Resolve the executable file. First read the path specified by the environment variable; if not found, then search from PATH. If not found, throw PermanentError.

## run_binary(argv, *, timeout_s, what) -> CompletedProcess

Run the subprocess with a hard timeout. A non-zero exit code is considered permanent (the input or calling method is problematic; retry does not change it); a timeout is considered transient (the machine may just be under high load). stdin is closed; stdout/stderr are all captured.
