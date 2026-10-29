#!/usr/bin/env python3
"""Filter the mic channel of a 16 kHz stereo WAV using an owner sample."""

# Tested API target: alkove==0.3.2 (PyPI Python package).
import argparse
import json
import os
import time
import wave


def read_wav(path, channels):
    with wave.open(str(path), "rb") as wav:
        if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getcomptype()) != (
            channels, 2, 16000, "NONE"
        ):
            raise ValueError(f"{path}: expected {channels}-channel 16 kHz, 16-bit PCM WAV")
        return wav.readframes(wav.getnframes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", help="stereo input WAV (left = mic, right = system)")
    parser.add_argument("sample", help="mono owner-only voice WAV")
    parser.add_argument("output", help="filtered stereo WAV")
    args = parser.parse_args()

    import alkove
    import numpy as np

    source = np.frombuffer(read_wav(args.input, 2), dtype="<i2").reshape(-1, 2)
    sample = np.frombuffer(read_wav(args.sample, 1), dtype="<i2")
    if len(source) == 0 or len(sample) < 24000:
        parser.error("input must be nonempty and owner sample must contain at least 1.5 seconds")

    models = alkove.Models.load(["timbre", "focus", "cue"])
    voiceprint = alkove.Timbre(models).embed(sample.astype(np.float32) / 32768)
    started = time.process_time()
    result = alkove.process(
        models, source[:, 0].astype(np.float32) / 32768,
        rate=16000, focus=True, voiceprint=voiceprint,
    )
    rtf = (time.process_time() - started) / (len(source) / 16000)
    if len(result.audio) != len(source):
        raise ValueError("Alkove output length differs from input")

    output = source.copy()
    output[:, 0] = np.clip(np.rint(np.asarray(result.audio) * 32768), -32768, 32767).astype("<i2")
    fd = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as file, wave.open(file, "wb") as wav:
        wav.setnchannels(2)
        wav.setsampwidth(2)
        wav.setframerate(16000)
        wav.writeframes(output.tobytes())
    print(json.dumps({"rtf": rtf}))


if __name__ == "__main__":
    main()
