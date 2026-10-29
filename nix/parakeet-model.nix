# NVIDIA Parakeet TDT 0.6B v3 (INT8, sherpa-onnx offline nemo transducer) as a
# fixed-output derivation, so the daemon never needs the network at run time.
{
  lib,
  fetchurl,
  runCommand,
  bzip2,
}:

let
  name = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
  self = runCommand "parakeet-model-tdt-0.6b-v3-int8"
    {
      nativeBuildInputs = [ bzip2 ];
      src = fetchurl {
        url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/${name}.tar.bz2";
        hash = "sha256-V5PQ/Tl8V3jSzyEmmU1Y6dVrG+fATRPHoVuxtOr7Fr8=";
      };
      passthru = {
        encoder = "${self}/encoder.int8.onnx";
        decoder = "${self}/decoder.int8.onnx";
        joiner = "${self}/joiner.int8.onnx";
        tokens = "${self}/tokens.txt";
      };
      meta = {
        description = "NVIDIA Parakeet TDT 0.6B v3, INT8 ONNX bundle for sherpa-onnx";
        homepage = "https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3";
        # Attribution required: "NVIDIA Parakeet TDT 0.6B v3, CC-BY-4.0".
        license = lib.licenses.cc-by-40;
      };
    }
    ''
      mkdir -p $out
      tar -xjf $src --strip-components=1 -C $out
    '';
in
self
