# VoxLock target-speaker voice isolation (INT8 chunk model) as packaged by sherpa-onnx, which adds
# the metadata its denoiser reads, as a fixed-output derivation: nothing is downloaded at run time.
{
  lib,
  fetchurl,
  runCommand,
  bzip2,
}:

let
  name = "sherpa-onnx-voxlock-int8";
  self =
    runCommand "voxlock-model-int8"
      {
        nativeBuildInputs = [ bzip2 ];
        src = fetchurl {
          url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speech-enhancement-models/${name}.tar.bz2";
          hash = "sha256-vLlH+ehnbT6WP2BIsAiFBchPCEO/3IjLJt3jJk2BVws=";
        };
        passthru = {
          enroll = "${self}/enroll.onnx";
          chunk = "${self}/chunk_int8.onnx";
        };
        meta = {
          description = "VoxLock target-speaker voice isolation, INT8 ONNX bundle for sherpa-onnx";
          homepage = "https://github.com/arbazsiddiqui/voxlock";
          license = lib.licenses.asl20;
        };
      }
      ''
        mkdir -p $out
        tar -xjf $src --strip-components=1 -C $out
      '';
in
self
