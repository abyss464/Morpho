# morpho-tts-adapter

Serves `tts.synthesize`. edge-tts produces mp3; ffmpeg transcodes it to mono Ogg
Opus at the requested bitrate with pinned, bit-exact settings.

See `adapters/README.md` for invocation, env vars and the ffmpeg requirement.
