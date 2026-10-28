#!/usr/bin/env python3
"""Write deterministic signal-only stereo 16 kHz fixtures (left = mic, right = system).

Usage: python3 -I synthetic.py OUTDIR
Files: echo.wav (system tone, delayed 120 ms and attenuated into the mic), doubletalk.wav
(echo plus an independent mic tone in the middle), silence.wav, and noise.wav.
"""
import math, os, random, struct, sys, wave

RATE = 16000


def tone(freq, seconds, amp=0.3):
    return [amp * math.sin(2 * math.pi * freq * i / RATE) for i in range(int(seconds * RATE))]


def noise(rng, seconds, amp=0.3):
    return [amp * rng.uniform(-1, 1) for _ in range(int(seconds * RATE))]


def delayed(x, ms, gain):
    pad = int(RATE * ms / 1000)
    return [0.0] * pad + [gain * v for v in x[: len(x) - pad]]


def write(path, left, right):
    with wave.open(path, "wb") as w:
        w.setnchannels(2); w.setsampwidth(2); w.setframerate(RATE)
        clip = lambda v: int(max(-1, min(1, v)) * 32767)
        w.writeframes(b"".join(struct.pack("<hh", clip(a), clip(b)) for a, b in zip(left, right)))


def main(out):
    rng = random.Random(1234)
    os.makedirs(out, exist_ok=True)
    system = noise(rng, 4)
    echo = delayed(system, 120, 0.4)
    write(f"{out}/echo.wav", echo, system)
    mic = tone(440, 1.5)
    double = [e + (mic[i - 16000] if 16000 <= i < 16000 + len(mic) else 0.0) for i, e in enumerate(echo)]
    write(f"{out}/doubletalk.wav", double, system)
    write(f"{out}/silence.wav", [0.0] * RATE * 4, [0.0] * RATE * 4)
    write(f"{out}/noise.wav", noise(rng, 4, 0.2), noise(rng, 4, 0.2))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "tools/accuracy/data")
