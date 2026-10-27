# A whisper.cpp model as a fixed-output derivation, so the speech-to-text
# service never needs the network at run time. Same approach as nixarchy-voice.
{
  lib,
  fetchurl,
  runCommand,
}:

let
  mkModel =
    { name, sha256 }:
    runCommand "whisper-model-${name}"
      {
        meta = {
          description = "whisper.cpp ggml-${name} model";
          homepage = "https://huggingface.co/ggerganov/whisper.cpp";
          license = lib.licenses.mit;
        };
        passthru.modelName = name;
      }
      ''
        mkdir -p $out
        cp ${
          fetchurl {
            url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-${name}.bin?download=true";
            inherit sha256;
          }
        } $out/ggml-${name}.bin
      '';
in
{
  "small.en" = mkModel {
    name = "small.en";
    sha256 = "0p8yqkwvpl9lyy43yajk305bps0v5z1qgyg0jwh35j7cb1nqs4y6";
  };
  "small" = mkModel {
    name = "small";
    sha256 = "0ywqxbziyp2bv72riyjpw4brk9v46d4cfbjfwqvvjrrq0srakqqv";
  };
  "large-v3-turbo" = mkModel {
    name = "large-v3-turbo";
    sha256 = "0sdwwblbfy9qjkjjxxvyfn47rx3y6pm1wfdcjfcidsrq9mvhziqz";
  };
}
