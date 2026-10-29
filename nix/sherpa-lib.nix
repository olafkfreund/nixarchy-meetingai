# The sherpa-onnx crate links `-lsherpa-onnx-c-api -lonnxruntime` from SHERPA_ONNX_LIB_DIR;
# nixpkgs ships the second library separately.
{ symlinkJoin, callPackage, onnxruntime }:
symlinkJoin {
  name = "sherpa-onnx-link-dir";
  paths = [ (callPackage ./sherpa-onnx.nix { }) onnxruntime ];
}
