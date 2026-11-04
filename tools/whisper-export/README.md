# NB-Whisper → sherpa-onnx int8

Exports NbAiLab's Norwegian Whisper to the layout `src/speech/stt/whisper_nb.rs`
reads (`encoder.int8.onnx`, `decoder.int8.onnx`, `tokens.txt`). Plan: step 4 of
`plan/2026-11-04-70-norwegian-transcription.md`. Heavy (torch, ~6 GB download); run once.

## Pins

| Input | Pin |
|---|---|
| `NbAiLab/nb-whisper-small` (shipped) | revision `e9bb5cb83cb74c96239fd506163aa97cff2fce4c`, `model.safetensors` plus `config.json` |
| sherpa-onnx export script | git tag `v1.12.40` (`d795ccd164bd2360ffda6c3431b1316e2c898be2`), `scripts/whisper/export-onnx.py` |
| python deps | `openai-whisper`, `torch`, `onnx`, `onnxruntime` (the script's `requirements.txt`) |

Other sizes (`medium`, `large` → base `large-v3`) work the same way at their own pinned revision.
`model.bin` in these repos is a CTranslate2 file and is never used: `export.py` reads the
safetensors weights, which cannot run code when loaded, and forces the legacy ONNX exporter
(`dynamo=False`) so the decoder keeps dynamic shapes.

## Command

```sh
W=$(mktemp -d) && cd "$W"
git clone --depth 1 --branch v1.12.40 https://github.com/k2-fsa/sherpa-onnx
R=https://huggingface.co/NbAiLab/nb-whisper-small/resolve/e9bb5cb83cb74c96239fd506163aa97cff2fce4c
mkdir hf && for f in model.safetensors config.json; do curl -fL -o hf/$f $R/$f; done
nix shell nixpkgs#python3Packages.{torch,onnx,onnxruntime,onnxscript,openai-whisper,safetensors} -c \
  python3 <repo>/tools/whisper-export/export.py "$W/sherpa-onnx/scripts/whisper" "$W/hf" small "$W/out"
```

The bundle `nix/whisper-nb-model.nix` fetches is `out/{encoder.int8.onnx,decoder.int8.onnx,tokens.txt}`
plus the Apache-2.0 `LICENSE` and a `README.md`, published as the `models-nb-whisper-small-e9bb5cb`
release asset.

## Licences

- NB-Whisper: Apache-2.0 (https://huggingface.co/NbAiLab/nb-whisper-large). Ship the licence with the release asset.
- OpenAI Whisper turbo (`nix/whisper-turbo-model.nix`): MIT.
- Silero VAD (`nix/silero-vad.nix`): MIT.
