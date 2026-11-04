# Silero VAD (sherpa-onnx's copy of silero_vad.onnx) as a fixed-output derivation.
# The Rust side accepts this file's path, or a directory containing silero_vad.onnx.
{ lib, fetchurl }:

fetchurl {
  name = "silero_vad.onnx";
  url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";
  hash = "sha256-niRJ4Qh0ltjUyrqQfyPgvT942R+lUkebucI6wJy7H9Y=";
  meta = {
    description = "Silero VAD ONNX model";
    homepage = "https://github.com/snakers4/silero-vad";
    license = lib.licenses.mit;
  };
}
