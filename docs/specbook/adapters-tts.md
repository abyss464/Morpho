# adapters/tts

7 specs.


============================================================
adapters/tts/src/morpho_tts/
============================================================


--- __init__.py ---

# TTS Adapter Package

entry point

export `OPS` (operation dispatch table) and `OP_SYNTHESIZE` (operation name constant).


--- __main__.py ---

# TTS Package Direct Run Entry Point

`python -m morpho_tts` is equivalent to the `tts-adapter` command-line entry point, which starts the TTS adapter process.


--- cli.py ---

# TTS Adapter CLI entry point

## main()
Start the TTS adapter process. Call the common database `morpho_adapter_common.run`, entering the main loop with the adapter name `"tts"` and this package's operation dispatch table. The process exit code is the return value of `run`.

Run method: `uv run --project adapters/tts tts-adapter`


--- engine.py ---

# edge-tts Synthesis Engine

Wraps edge-tts's network call, returns raw MP3 bytes, and classifies all exceptions into adapter error types (permanent/transient/rate-limited).

## engine_version() → string
Returns `"edge-tts/<version>"`, used for writing the TTS record's `engine_ver` field.

## validate_prosody(rate, pitch, volume)
Validates the format of rate, pitch, and volume parameters. Rate and volume must be of the form `"+0%"` or `"-10%"`, pitch of the form `"+0Hz"` or `"-5Hz"`. If the format is incorrect, throws a PermanentError directly without making a network request.

## synthesize_mp3(text, voice, ...) → bytes
Calls edge-tts to synthesize speech and returns MP3 bytes. Optional parameters: rate, pitch, volume (prosody control) and timeout_s (timeout in seconds). The timeout defaults to 45 seconds, and can be overridden via the environment variable `MORPHO_TTS_TIMEOUT_S`. On synthesis failure, throws a classified AdapterError subclass.

## classify(exc) → AdapterError
Maps any exception to RateLimitedError, TransientError, or PermanentError. Determines the category based on the exception name and keywords in the message—rate limiting (429/rate limit, etc.), permanent failure (NoAudioReceived/ValueError, etc.), transient failure (connection error/timeout, etc.).

## voice_catalog() → list of dictionaries
Gets edge-tts's full voice list. For operational viewing only; morphod does not call it.

## environment variable
- `MORPHO_TTS_TIMEOUT_S`: The synthesis timeout in seconds, defaults to 45.

## constraint
- Error classification is based on matching the exception name and message text. Bypassing this module and throwing exceptions directly will cause morphod to fail to retry or discard correctly.


--- oggopus.py ---

# Ogg Opus Container Parsing

Extract precise duration from Ogg Opus byte streams without depending on ffprobe.

## OpusInfo (dataclass)
The parse result contains:
- `channels`: number of channels
- `pre_skip`: number of samples pre-skipped by the encoder
- `final_granule`: granule position of the last page
- `sample_count`: number of valid samples (final_granule minus pre_skip)
- `duration_ms`: duration in milliseconds, precisely calculated at a 48 kHz sample rate

## parse(data) → OpusInfo
Takes the full bytes of an Ogg Opus file, iterates over all Ogg pages, and returns an OpusInfo. Raises OggParseError if the data is invalid.

## parse_file(path) → OpusInfo
Reads and parses from a file path.

## OggParseError
Subclass of ValueError, indicating that the byte stream is not a valid Ogg Opus file.

## Constraint
- Only Ogg Opus format can be parsed; other Ogg containers (e.g., Vorbis) will cause an error.


--- ops.py ---

# TTS Operation Dispatch

This adapter's only operation: `tts.synthesize`.

## synthesize(params) → dictionary

Synthesizes text to a mono Ogg Opus audio file.

**Required parameters:**
- `text`: Text to synthesize, limited to 5000 characters
- `voice`: edge-tts voice name
- `out`: output file path

**Optional parameters:**
- `rate`: speaking rate, default `"+0%"`
- `pitch`: pitch, default `"+0Hz"`
- `volume`: volume, default `"+0%"`
- `format`: output format; currently only `"ogg_opus"` is supported
- `bitrate_kbps`: bitrate, default 32, range 6–510

**Returns:**
- `duration_ms`: synthesized audio duration in milliseconds
- `engine_ver`: engine version number

**Side effect:** writes an Ogg Opus file to the `out` path. The write uses a temp-file-rename method, so it either writes completely or leaves no residual file.

## OPS (Dispatch Table)

`{"tts.synthesize": synthesize}`, registered with the adapter's main loop.

## Constraints

- Text over 5000 characters is directly rejected, and no network request is made.
- Without ffmpeg, it fails permanently; retrying is futile.


--- transcode.py ---

# MP3 → Ogg Opus Transcoding

Uses ffmpeg to transcode MP3 into mono 48kHz Ogg Opus. All encoding parameters are fixed, ensuring the same input produces identical byte output.

## find_ffmpeg() → string
Locates the ffmpeg executable path. First checks the `MORPHO_FFMPEG` environment variable; otherwise, searches in PATH. If not found, raises PermanentError.

## to_ogg_opus(src, dst, bitrate_kbps, ffmpeg=None)
Transcodes src (MP3 file) into dst (Ogg Opus file). An ffmpeg path may be specified; otherwise, it is located automatically. Transcoding timeout defaults to 30 seconds, overridable via the `MORPHO_TTS_FFMPEG_TIMEOUT_S` environment variable. If ffmpeg lacks the libopus encoder, raises a clear PermanentError.

## build_argv(ffmpeg, src, dst, bitrate_kbps) → stringlist
Builds the ffmpeg command-line arguments. Fixed settings: mono, 48kHz, VBR enabled, bitexact flag (removes metadata), compression_level 10.

## Environment Variables
- `MORPHO_FFMPEG`: ffmpeg path override
- `MORPHO_TTS_FFMPEG_TIMEOUT_S`: transcoding timeout in seconds, default 30

## Constraints
- Changing any fixed parameter in build_argv will alter the output bytes, invalidating content-addressed storage. After any modification, the `TRANSCODER_VER` constant must be updated in sync.
