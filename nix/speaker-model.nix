# 3D-Speaker CAM++ speaker-embedding model (sherpa-onnx ONNX export) as a fixed-output
# derivation, so the daemon never needs the network at run time. The file itself is the output.
# Source: ModelScope iic/speech_campplus_sv_zh_en_16k-common_advanced (Apache-2.0).
{ lib, fetchurl }:

fetchurl {
  # "recongition" is the real (misspelled) release tag.
  url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx";
  hash = "sha256-qjz8FpY6EFhqk5P1A11ta1fpjTWLNH+AwqML9PAM66I=";
  meta = {
    description = "3D-Speaker CAM++ (zh/en, 16 kHz) speaker-embedding model for sherpa-onnx";
    homepage = "https://modelscope.cn/models/iic/speech_campplus_sv_zh_en_16k-common_advanced";
    license = lib.licenses.asl20;
  };
}
