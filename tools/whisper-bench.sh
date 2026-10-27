#!/usr/bin/env bash
# Benchmark whisper.cpp (Vulkan) models on the LibriSpeech dummy dev set:
# WER, real-time factor and per-utterance latency. Needs nix, curl, python3.
# Usage: tools/whisper-bench.sh [model ...]   (default: small.en medium.en large-v3-turbo)
# Env: WHISPER_BENCH_CACHE (default ~/.cache/whisper-bench)
set -euo pipefail

CACHE="${WHISPER_BENCH_CACHE:-$HOME/.cache/whisper-bench}"
mkdir -p "$CACHE/models" "$CACHE/wav"
MODELS=("$@")
[ ${#MODELS[@]} -gt 0 ] || MODELS=(small.en medium.en large-v3-turbo)

SERVER_PKG=$(nix build nixpkgs#whisper-cpp-vulkan --no-link --print-out-paths)
export WHISPER_SERVER="$SERVER_PKG/bin/whisper-server"
if command -v ffmpeg >/dev/null; then
  export FFMPEG=ffmpeg
else
  export FFMPEG="$(nix build nixpkgs#ffmpeg --no-link --print-out-paths | head -1)/bin/ffmpeg"
fi
export WHISPER_VERSION="$(basename "$SERVER_PKG")"
export BENCH_CACHE="$CACHE"

for m in "${MODELS[@]}"; do
  f="$CACHE/models/ggml-$m.bin"
  if [ ! -s "$f" ]; then
    curl -fsSL --retry 3 -o "$f.part" "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-$m.bin"
    mv "$f.part" "$f"
  fi
done

exec python3 -I - "${MODELS[@]}" <<'PY'
import json, os, re, socket, statistics, subprocess, sys, time, urllib.parse, urllib.request, uuid, wave

CACHE = os.environ["BENCH_CACHE"]
DS = "hf-internal-testing/librispeech_asr_dummy"

def norm(s):
    return re.sub(r"\s+", " ", re.sub(r"[^a-z0-9' ]", " ", s.lower().replace("-", " "))).strip().split()

def wer(ref, hyp):
    r, h = norm(ref), norm(hyp)
    d = list(range(len(h) + 1))
    for i, rw in enumerate(r, 1):
        p, d[0] = d[0], i
        for j, hw in enumerate(h, 1):
            p, d[j] = d[j], min(d[j] + 1, d[j - 1] + 1, p + (rw != hw))
    return d[-1], len(r)

def selftest():
    assert wer("The cat sat.", "the cat sat") == (0, 3)
    assert wer("a b c d", "a x c") == (2, 4)
    assert wer("hello", "") == (1, 1)

def get(url):
    return urllib.request.urlopen(url, timeout=120).read()

def dataset():
    idx = os.path.join(CACHE, "wav", "index.json")
    if os.path.exists(idx):
        return json.load(open(idx))
    out, off = [], 0
    while True:
        q = urllib.parse.urlencode(dict(dataset=DS, config="clean", split="validation", offset=off, length=100))
        rows = json.loads(get("https://datasets-server.huggingface.co/rows?" + q))["rows"]
        if not rows:
            break
        for r in rows:
            row = r["row"]
            flac = os.path.join(CACHE, "wav", row["id"] + ".flac")
            wav = flac[:-5] + ".wav"
            open(flac, "wb").write(get(row["audio"][0]["src"]))
            subprocess.run([os.environ["FFMPEG"], "-v", "error", "-y", "-i", flac, "-ar", "16000", "-ac", "1", wav], check=True)
            os.remove(flac)
            out.append(dict(wav=wav, text=row["text"]))
        off += len(rows)
    json.dump(out, open(idx, "w"))
    return out

def post(port, wav):
    b = uuid.uuid4().hex
    body = b"".join([
        f'--{b}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'.encode()
        for k, v in dict(temperature="0", response_format="json", language="en").items()
    ]) + f'--{b}\r\nContent-Disposition: form-data; name="file"; filename="a.wav"\r\nContent-Type: audio/wav\r\n\r\n'.encode() \
        + open(wav, "rb").read() + f"\r\n--{b}--\r\n".encode()
    req = urllib.request.Request(f"http://127.0.0.1:{port}/inference", body,
                                 {"Content-Type": f"multipart/form-data; boundary={b}"})
    t = time.perf_counter()
    text = json.load(urllib.request.urlopen(req, timeout=300))["text"]
    return text, time.perf_counter() - t

def dur(wav):
    with wave.open(wav) as w:
        return w.getnframes() / w.getframerate()

def run(model, data):
    s = socket.socket(); s.bind(("127.0.0.1", 0)); port = s.getsockname()[1]; s.close()
    log = open(os.path.join(CACHE, f"server-{model}.log"), "w")
    srv = subprocess.Popen([os.environ["WHISPER_SERVER"], "-m", f"{CACHE}/models/ggml-{model}.bin",
                            "--host", "127.0.0.1", "--port", str(port)], stdout=log, stderr=subprocess.STDOUT)
    try:
        for _ in range(600):
            try:
                get(f"http://127.0.0.1:{port}/"); break
            except Exception:
                assert srv.poll() is None, "whisper-server exited"
                time.sleep(0.5)
        post(port, data[0]["wav"])  # warm-up, not measured
        errs = words = 0; lat = []; audio = 0.0
        for u in data:
            text, t = post(port, u["wav"])
            e, n = wer(u["text"], text); errs += e; words += n
            lat.append(t); audio += dur(u["wav"])
        lat.sort()
        return dict(model=model, wer=100 * errs / words, rtf=sum(lat) / audio,
                    p50=statistics.median(lat), p95=lat[min(len(lat) - 1, int(0.95 * len(lat) + 0.999999) - 1)], n=len(lat))
    finally:
        srv.terminate(); srv.wait()
        log.close()

selftest()
data = dataset()
print(f"{len(data)} utterances, {sum(dur(u['wav']) for u in data):.0f} s audio; {os.environ['WHISPER_VERSION']}", file=sys.stderr)
print("| model | WER % | RTF | p50 s | p95 s | utterances |\n|---|---|---|---|---|---|")
for m in sys.argv[1:]:
    r = run(m, data)
    print(f"| {m} | {r['wer']:.2f} | {r['rtf']:.3f} | {r['p50']:.2f} | {r['p95']:.2f} | {r['n']} |", flush=True)
PY
