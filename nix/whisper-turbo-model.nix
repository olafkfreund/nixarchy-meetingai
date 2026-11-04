# OpenAI Whisper large-v3-turbo (INT8, sherpa-onnx offline whisper) as a fixed-output
# derivation, so the daemon never needs the network at run time.
{
  lib,
  fetchurl,
  runCommand,
  bzip2,
}:

let
  self = runCommand "whisper-turbo-model-int8"
    {
      nativeBuildInputs = [ bzip2 ];
      src = fetchurl {
        url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-turbo.tar.bz2";
        hash = "sha256-sRrLvNZgtEqODfM3JP61qqcJz2Vmjygj1Z9lYxJUTyI=";
      };
      passthru = {
        encoder = "${self}/turbo-encoder.int8.onnx";
        decoder = "${self}/turbo-decoder.int8.onnx";
        tokens = "${self}/turbo-tokens.txt";
      };
      meta = {
        description = "OpenAI Whisper turbo, INT8 ONNX bundle for sherpa-onnx";
        homepage = "https://github.com/openai/whisper";
        license = lib.licenses.mit;
      };
    }
    ''
      mkdir -p $out
      tar -xjf $src --strip-components=1 -C $out
    '';
in
self
