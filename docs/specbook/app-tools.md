# app/tools

1 specs.


============================================================
app/tools/
============================================================


--- gen_sfx.py ---

# Sound Effect Generation Script

Pure standard library synthesizer that generates placeholder sound effect files for Wave 1. When replacing with professional sound effects later, you only need to replace the files — no code changes required.

## Usage

```
python3 tools/gen_sfx.py [--out DIR] [--rate HZ]
```

- `--out` — output directory, defaults to `app/src/main/assets/sfx/`
- `--rate` — sample rate, default 44100 Hz

## Output

Generates seven 16-bit mono PCM WAV files, each corresponding to one UI event:

- tap — tap: very light click
- correct — correct: short xylophone ascending tone
- wrong — wrong: muted low tone (deliberately not harsh)
- promote — mode promotion: two-note ascending tone
- group_complete — group complete: three-note trumpet-style ascending tone
- review_done — review finished: soft bell tone
- streak — streak: a series of rising chime notes

Each file must not exceed 100 KB; if any file exceeds that, the script exits with a non-zero status.
