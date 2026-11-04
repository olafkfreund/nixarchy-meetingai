# NB-Whisper → sherpa-onnx int8

Exports NbAiLab's Norwegian Whisper to the layout `src/speech/stt/whisper_nb.rs`
reads (`encoder.int8.onnx`, `decoder.int8.onnx`, `tokens.txt`). Plan: step 4 of
`plan/2026-11-04-70-norwegian-transcription.md`. Heavy (torch, ~6 GB download); run once.

## Pins

| Input | Pin |
|---|---|
| `NbAiLab/nb-whisper-large` | revision `8c6249fdeeb4dcd05e5735a4c39640607eb6e4ac`, file `model.bin` (6174511117 bytes, sha256 `91a2575f6e703d0bf6d8ce228a490c40e2404e6275932c20ff137382f35f726d`) |
| sherpa-onnx export script | git tag `v1.12.40` (`d795ccd164bd2360ffda6c3431b1316e2c898be2`), `scripts/whisper/export-onnx.py` |
| python deps | `openai-whisper`, `torch`, `onnx`, `onnxruntime` (the script's `requirements.txt`) |

For the smaller `nb-whisper-medium`, pin its own revision the same way and pass `medium` below.

## Command

```sh
W=$(mktemp -d) && cd "$W"
git clone --depth 1 --branch v1.12.40 https://github.com/k2-fsa/sherpa-onnx
curl -L -o model.bin \
  https://huggingface.co/NbAiLab/nb-whisper-large/resolve/8c6249fdeeb4dcd05e5735a4c39640607eb6e4ac/model.bin
echo "91a2575f6e703d0bf6d8ce228a490c40e2404e6275932c20ff137382f35f726d  model.bin" | sha256sum -c
nix shell nixpkgs#python3Packages.{torch,onnx,onnxruntime,openai-whisper} -c \
  python3 <repo>/tools/whisper-export/export.py "$W/sherpa-onnx/scripts/whisper" "$W/model.bin" large-v3 "$W/out"
```

(Or a venv with `pip install openai-whisper onnx onnxruntime torch`.) `large-v3` is the
base size, which sets 128 mel bins: NB-Whisper large is fine-tuned from large-v3 (its
`preprocessor_config.json` has `feature_size: 128`).

**Unverified:** that `model.bin` is in OpenAI checkpoint format (what `whisper.load_model`
reads). If loading fails with missing/unexpected keys it is the Hugging Face layout:
convert it first (inverse of transformers' `convert_openai_to_hf.py`) or treat the export
as failed (plan step 4: the gate then compares turbo against Parakeet only).

## Licences

- NB-Whisper: Apache-2.0 (https://huggingface.co/NbAiLab/nb-whisper-large). Ship the licence with the release asset.
- OpenAI Whisper turbo (`nix/whisper-turbo-model.nix`): MIT.
- Silero VAD (`nix/silero-vad.nix`): MIT.
