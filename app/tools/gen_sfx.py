#!/usr/bin/env python3
"""Synthesise Morpho's UI sound effects.

Wave 1 ships generated placeholder tones (docs/contracts/app-design.md, "Sound
design"): real sound design swaps the files later without touching any code.

Standard library only -- no numpy, no downloads. Writes 16-bit mono PCM WAV files
into ``app/src/main/assets/sfx/`` where ``SoundManager`` preloads them into a
``SoundPool`` at startup.

Usage:
    python3 tools/gen_sfx.py [--out DIR] [--rate HZ]

The seven events and their intended character:

    tap             soft tick, barely-there
    correct         short marimba ding, upward
    wrong           muted low thud (never harsh)
    promote         two-note rise
    group_complete  three-note fanfare
    review_done     soft chime
    streak          sparkle
"""

from __future__ import annotations

import argparse
import math
import os
import random
import struct
import sys
import wave

DEFAULT_RATE = 44100
DEFAULT_OUT = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "app", "src", "main", "assets", "sfx"
)

# Equal-tempered pitches used by the melodic cues.
NOTES = {
    "C4": 261.63,
    "E4": 329.63,
    "G4": 392.00,
    "A4": 440.00,
    "C5": 523.25,
    "D5": 587.33,
    "E5": 659.25,
    "G5": 783.99,
    "A5": 880.00,
    "C6": 1046.50,
    "E6": 1318.51,
    "G6": 1567.98,
}


class Buffer:
    """A mono float sample buffer with sub-sample-free mixing helpers."""

    def __init__(self, rate: int, seconds: float) -> None:
        self.rate = rate
        self.data = [0.0] * int(rate * seconds)

    def __len__(self) -> int:
        return len(self.data)

    def add(self, index: int, value: float) -> None:
        if 0 <= index < len(self.data):
            self.data[index] += value


def envelope(n: int, total: int, attack: float, decay: float, curve: float = 1.0) -> float:
    """Percussive attack/decay envelope evaluated at sample ``n``."""
    t = n / total if total else 0.0
    if t < attack:
        return (t / attack) ** 0.6 if attack else 1.0
    fall = (t - attack) / max(decay, 1e-6)
    return math.exp(-curve * 5.0 * fall)


def tone(
    buf: Buffer,
    start_s: float,
    dur_s: float,
    freq: float,
    gain: float,
    partials=((1.0, 1.0), (2.0, 0.28), (3.0, 0.11)),
    attack: float = 0.005,
    decay: float = 1.0,
    curve: float = 1.0,
    detune_cents: float = 0.0,
) -> None:
    """Adds an additive-synth note. ``partials`` is a list of (ratio, amplitude)."""
    rate = buf.rate
    start = int(start_s * rate)
    count = int(dur_s * rate)
    if count <= 0:
        return
    f0 = freq * (2.0 ** (detune_cents / 1200.0))
    norm = sum(a for _, a in partials) or 1.0
    for n in range(count):
        env = envelope(n, count, attack / max(dur_s, 1e-6), decay, curve)
        if env < 1e-5:
            continue
        s = 0.0
        for ratio, amp in partials:
            # Higher partials die away faster -- what makes a struck bar sound struck.
            s += amp * math.sin(2.0 * math.pi * f0 * ratio * n / rate) * (env ** (1.0 + 0.9 * ratio))
        buf.add(start + n, gain * s / norm)


def noise_burst(
    buf: Buffer,
    start_s: float,
    dur_s: float,
    gain: float,
    lowpass: float = 0.35,
    seed: int = 1,
) -> None:
    """One-pole low-passed noise: the body of a tick or a thud."""
    rate = buf.rate
    rng = random.Random(seed)
    start = int(start_s * rate)
    count = int(dur_s * rate)
    prev = 0.0
    for n in range(count):
        white = rng.uniform(-1.0, 1.0)
        prev += lowpass * (white - prev)
        env = envelope(n, count, 0.02, 1.0, curve=1.6)
        buf.add(start + n, gain * prev * env)


def soft_clip(x: float) -> float:
    """Gentle saturation so nothing in the set can ever sound harsh."""
    return math.tanh(1.35 * x) / math.tanh(1.35)


def normalise(buf: Buffer, peak: float) -> None:
    current = max((abs(v) for v in buf.data), default=0.0)
    if current < 1e-9:
        return
    scale = peak / current
    buf.data = [soft_clip(v * scale) for v in buf.data]


def fade_out(buf: Buffer, seconds: float = 0.01) -> None:
    """Kills the click a hard buffer end would otherwise produce."""
    n = min(int(buf.rate * seconds), len(buf))
    for i in range(n):
        buf.data[len(buf) - n + i] *= 1.0 - i / n


def write_wav(path: str, buf: Buffer) -> int:
    fade_out(buf)
    frames = bytearray()
    for value in buf.data:
        clamped = max(-1.0, min(1.0, value))
        frames += struct.pack("<h", int(clamped * 32767))
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(buf.rate)
        w.writeframes(bytes(frames))
    return os.path.getsize(path)


# --------------------------------------------------------------------------- cues

def make_tap(rate: int) -> Buffer:
    """Barely-there tick: a short filtered noise transient with a faint pitch."""
    buf = Buffer(rate, 0.06)
    noise_burst(buf, 0.0, 0.035, gain=0.5, lowpass=0.55, seed=11)
    tone(buf, 0.0, 0.05, NOTES["A5"], gain=0.22, partials=((1.0, 1.0),), attack=0.001, curve=2.4)
    normalise(buf, 0.35)
    return buf


def make_correct(rate: int) -> Buffer:
    """Short marimba ding, upward: two struck notes a fifth apart."""
    buf = Buffer(rate, 0.42)
    marimba = ((1.0, 1.0), (3.9, 0.35), (9.2, 0.12))
    tone(buf, 0.0, 0.26, NOTES["E5"], gain=0.9, partials=marimba, attack=0.003, curve=1.3)
    tone(buf, 0.075, 0.34, NOTES["A5"], gain=0.8, partials=marimba, attack=0.003, curve=1.1)
    tone(buf, 0.075, 0.34, NOTES["A5"], gain=0.2, partials=((2.0, 1.0),), attack=0.004, curve=1.4)
    normalise(buf, 0.82)
    return buf


def make_wrong(rate: int) -> Buffer:
    """Muted low thud. Deliberately dull: no bite, no dissonant partials."""
    buf = Buffer(rate, 0.30)
    tone(buf, 0.0, 0.26, 132.0, gain=1.0, partials=((1.0, 1.0), (1.5, 0.16)), attack=0.006, curve=1.5)
    tone(buf, 0.0, 0.16, 88.0, gain=0.55, partials=((1.0, 1.0),), attack=0.008, curve=1.8)
    noise_burst(buf, 0.0, 0.06, gain=0.18, lowpass=0.10, seed=23)
    normalise(buf, 0.62)
    return buf


def make_promote(rate: int) -> Buffer:
    """Two-note rise marking a mode promotion."""
    buf = Buffer(rate, 0.42)
    bell = ((1.0, 1.0), (2.0, 0.30), (4.2, 0.10))
    tone(buf, 0.0, 0.20, NOTES["C5"], gain=0.85, partials=bell, attack=0.004, curve=1.5)
    tone(buf, 0.11, 0.30, NOTES["G5"], gain=0.9, partials=bell, attack=0.004, curve=1.2)
    normalise(buf, 0.78)
    return buf


def make_group_complete(rate: int) -> Buffer:
    """Three-note fanfare: C - E - G, each note ringing into the next."""
    buf = Buffer(rate, 0.95)
    bell = ((1.0, 1.0), (2.0, 0.34), (3.0, 0.16), (5.4, 0.07))
    for i, note in enumerate(("C5", "E5", "G5")):
        tone(buf, 0.10 * i, 0.55 - 0.06 * i, NOTES[note], gain=0.8, partials=bell,
             attack=0.004, curve=0.9)
    # A high shimmer on the last note so the fanfare lifts instead of stopping.
    tone(buf, 0.20, 0.62, NOTES["C6"], gain=0.34, partials=((1.0, 1.0), (2.0, 0.2)),
         attack=0.010, curve=0.8)
    normalise(buf, 0.86)
    return buf


def make_review_done(rate: int) -> Buffer:
    """Soft chime: one warm note with a slow attack and a long tail."""
    buf = Buffer(rate, 0.85)
    chime = ((1.0, 1.0), (2.0, 0.24), (3.0, 0.10), (4.7, 0.05))
    tone(buf, 0.0, 0.80, NOTES["G5"], gain=0.9, partials=chime, attack=0.030, curve=0.7)
    tone(buf, 0.02, 0.70, NOTES["D5"], gain=0.35, partials=chime, attack=0.035, curve=0.7)
    normalise(buf, 0.68)
    return buf


def make_streak(rate: int) -> Buffer:
    """Sparkle: a scatter of tiny high bells climbing in pitch."""
    buf = Buffer(rate, 0.70)
    sparkle = ((1.0, 1.0), (2.7, 0.22))
    steps = ("C5", "E5", "G5", "C6", "E6", "G6")
    for i, note in enumerate(steps):
        tone(
            buf,
            0.045 * i,
            0.34 - 0.03 * i,
            NOTES[note],
            gain=0.55 + 0.05 * i,
            partials=sparkle,
            attack=0.002,
            curve=1.4,
            detune_cents=(-6 if i % 2 else 6),
        )
    normalise(buf, 0.70)
    return buf


CUES = {
    "tap": make_tap,
    "correct": make_correct,
    "wrong": make_wrong,
    "promote": make_promote,
    "group_complete": make_group_complete,
    "review_done": make_review_done,
    "streak": make_streak,
}

MAX_BYTES = 100 * 1024  # docs/contracts/app-design.md: SFX files stay under 100 KB.


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default=DEFAULT_OUT, help="output directory")
    parser.add_argument("--rate", type=int, default=DEFAULT_RATE, help="sample rate in Hz")
    args = parser.parse_args(argv)

    out_dir = os.path.normpath(args.out)
    os.makedirs(out_dir, exist_ok=True)

    failures = []
    for name, builder in sorted(CUES.items()):
        path = os.path.join(out_dir, f"{name}.wav")
        size = write_wav(path, builder(args.rate))
        flag = "" if size <= MAX_BYTES else "  !! over budget"
        if size > MAX_BYTES:
            failures.append(name)
        print(f"{name:<16} {size / 1024:6.1f} KB  {path}{flag}")

    if failures:
        print(f"\nover the {MAX_BYTES // 1024} KB budget: {', '.join(failures)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
