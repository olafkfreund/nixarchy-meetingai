#!/usr/bin/env python3
"""Build the voice-check set from LibriSpeech test-clean WAVs (plan #31 step 7).

Usage: python3 -I voice_set.py WAVDIR OUTDIR [EXTRA_ROOM.wav ...]
WAVDIR holds <speaker>/<chapter>/<utt>.wav (16 kHz mono). Each speaker is the owner once:
12 s of their first chapter is the enrollment (enroll/<speaker>.wav); their other utterances,
up to 10 s each, are owner trials, clean and with another speaker's bleed at -12 and -6 dB;
20 utterances of other speakers, and each EXTRA_ROOM file cut in 6 s pieces, are room trials.
Writes OUTDIR/enroll.tsv and OUTDIR/trials.tsv for examples/voice_eval.rs.
"""
import os, random, sys, wave

RATE = 16000


def frames(path):
    with wave.open(path, "rb") as w:
        assert w.getframerate() == RATE and w.getnchannels() == 1, path
        return w.readframes(w.getnframes())


def write(path, data):
    with wave.open(path, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(RATE); w.writeframes(data)


def main(wavdir, out, extra):
    rng = random.Random(31)
    os.makedirs(f"{out}/enroll", exist_ok=True); os.makedirs(f"{out}/cut", exist_ok=True)
    utts = {}
    for spk in sorted(os.listdir(wavdir)):
        for ch in sorted(os.listdir(f"{wavdir}/{spk}")):
            for u in sorted(os.listdir(f"{wavdir}/{spk}/{ch}")):
                utts.setdefault(spk, []).append((ch, f"{wavdir}/{spk}/{ch}/{u}"))
    rooms = []
    for n, path in enumerate(extra):
        data = frames(path)
        step = 6 * RATE * 2
        for i in range(0, len(data) - step, step):
            write(f"{out}/cut/extra{n}-{i // step}.wav", data[i:i + step]); rooms.append(f"{out}/cut/extra{n}-{i // step}.wav")
    enroll, trials = [], []
    speakers = sorted(utts)
    for spk in speakers:
        first = utts[spk][0][0]
        data, used = b"", set()
        for ch, path in utts[spk]:
            if ch == first and len(data) < 12 * RATE * 2:
                data += frames(path); used.add(path)
        write(f"{out}/enroll/{spk}.wav", data[: 12 * RATE * 2]); enroll.append(f"{spk}\t{out}/enroll/{spk}.wav")
        others = [s for s in speakers if s != spk]
        for ch, path in utts[spk]:
            if path in used:
                continue
            short = f"{out}/cut/{os.path.basename(path)}"
            if not os.path.exists(short):
                write(short, frames(path)[: 10 * RATE * 2])
            bleed = rng.choice(utts[rng.choice(others)])[1]
            trials.append(f"{spk}\towner\tclean\t{short}")
            for db in (-12, -6):
                trials.append(f"{spk}\towner\tbleed{db}\t{short}\t{bleed}\t{db}")
        for _ in range(20):
            path = rng.choice(utts[rng.choice(others)])[1]
            trials.append(f"{spk}\troom\tother-speaker\t{path}")
        for path in rooms:
            trials.append(f"{spk}\troom\textra\t{path}")
    with open(f"{out}/enroll.tsv", "w") as f: f.write("\n".join(enroll) + "\n")
    with open(f"{out}/trials.tsv", "w") as f: f.write("\n".join(trials) + "\n")
    print(f"{len(enroll)} owners, {len(trials)} trials")


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2], sys.argv[3:])
