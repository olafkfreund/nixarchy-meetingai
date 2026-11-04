# NB-Whisper small (National Library of Norway, Apache-2.0) exported to sherpa-onnx INT8 for the
# local Norwegian engine (#70). The export is a release asset of this repo, reproducible with
# tools/whisper-export/ (pinned NB-Whisper revision and sherpa-onnx export script).
{
  lib,
  fetchurl,
  runCommand,
  bzip2,
}:

let
  self = runCommand "whisper-nb-small-int8"
    {
      nativeBuildInputs = [ bzip2 ];
      src = fetchurl {
        url = "https://github.com/olafkfreund/nixarchy-meetingai/releases/download/models-nb-whisper-small-e9bb5cb/nb-whisper-small-sherpa-int8.tar.bz2";
        hash = "sha256-ZMX2xkFoL4Oi7UrkhiJfBYQ7otAXlGy3rtPJz0DC4kk=";
      };
      passthru = {
        encoder = "${self}/encoder.int8.onnx";
        decoder = "${self}/decoder.int8.onnx";
        tokens = "${self}/tokens.txt";
      };
      meta = {
        description = "NB-Whisper small (Norwegian), INT8 ONNX bundle for sherpa-onnx";
        homepage = "https://huggingface.co/NbAiLab/nb-whisper-small";
        license = lib.licenses.asl20;
      };
    }
    ''
      mkdir -p $out
      tar -xjf $src --strip-components=1 -C $out
    '';
in
self
