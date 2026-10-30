#!/usr/bin/env python3
"""Build stereo test calls from LibriSpeech test-clean WAVs (plan #32 step 5, owner's choice).

Usage: python3 -I libri_mix.py WAVDIR ENROLLDIR OUTDIR
For every 4th speaker (the owner): a call of 10 turns, them and me alternating, two of the
"them" turns spoken over by the owner (double talk). Left = mic: the owner, plus the remote
speech from the speakers 120 ms late at -12 dB. Right = system: the remote speech.
Writes OUTDIR/<speaker>.wav and .txt (score.py reference) and copies the owner's sample
from ENROLLDIR/<speaker>.wav (made by voice_set.py on the #31 branch, never a trial utterance).
"""
import array, os, random, shutil, sys, wave

RATE = 16000
GAP = int(0.8 * RATE)
DELAY = int(0.12 * RATE)
BLEED = 10 ** (-12 / 20)


def read(path):
    with wave.open(path, "rb") as w:
        assert w.getframerate() == RATE and w.getnchannels() == 1, path
        return array.array("h", w.readframes(w.getnframes()))


def text(trans, path):
    return trans[os.path.basename(path)[:-4]].lower()


def main(wavdir, enrolldir, out):
    rng = random.Random(32)
    os.makedirs(out, exist_ok=True)
    trans = {}
    for root, _, files in os.walk(os.path.join(wavdir, "..", "LibriSpeech")):
        for f in files:
            if f.endswith(".trans.txt"):
                for line in open(os.path.join(root, f)):
                    k, _, v = line.strip().partition(" ")
                    trans[k] = v
    utts = {}
    for spk in sorted(os.listdir(wavdir)):
        for ch in sorted(os.listdir(f"{wavdir}/{spk}")):
            utts.setdefault(spk, []).extend(f"{wavdir}/{spk}/{ch}/{u}" for u in sorted(os.listdir(f"{wavdir}/{spk}/{ch}")))
    speakers = sorted(utts)
    for spk in speakers[::4]:
        mine = [u for u in utts[spk][6:] if len(read(u)) < 12 * RATE]  # the first ones made the sample
        remote = rng.choice([s for s in speakers if s != spk])
        theirs = [u for u in utts[remote] if len(read(u)) < 12 * RATE]
        left, right, ref, t = [], [], [], 0
        def place(buf, at, samples, gain=1.0):
            if len(buf) < at + len(samples):
                buf.extend([0.0] * (at + len(samples) - len(buf)))
            for i, s in enumerate(samples):
                buf[at + i] += gain * s / 32768
        for turn in range(10):
            if turn % 2 == 0:
                u = theirs[turn // 2]; s = read(u)
                place(right, t, s); place(left, t + DELAY, s, BLEED)
                ref.append(f"them: {text(trans, u)}")
                if turn in (4, 8):  # the owner talks over the remote half way in
                    over = mine.pop(); m = read(over)
                    place(left, t + len(s) // 2, m); ref.append(f"me: {text(trans, over)}")
                    t += max(0, len(s) // 2 + len(m) - len(s))  # the turn ends when the owner stops
                t += len(s) + GAP
            else:
                u = mine.pop(0); s = read(u)
                place(left, t, s); ref.append(f"me: {text(trans, u)}")
                t += len(s) + GAP
        n = max(len(left), len(right))
        left += [0.0] * (n - len(left)); right += [0.0] * (n - len(right))
        clip = lambda v: max(-32768, min(32767, int(v * 32767)))
        with wave.open(f"{out}/{spk}.wav", "wb") as w:
            w.setnchannels(2); w.setsampwidth(2); w.setframerate(RATE)
            w.writeframes(array.array("h", (clip(v) for pair in zip(left, right) for v in pair)).tobytes())
        open(f"{out}/{spk}.txt", "w").write("\n".join(ref) + "\n")
        shutil.copy(f"{enrolldir}/{spk}.wav", f"{out}/{spk}-sample.wav")
        print(spk, f"{n / RATE:.0f}s")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(*sys.argv[1:])
