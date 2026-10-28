# The sherpa-onnx crate links `-lsherpa-onnx-c-api -lonnxruntime` from SHERPA_ONNX_LIB_DIR;
# nixpkgs ships the second library separately.
{ symlinkJoin, sherpa-onnx, onnxruntime }:
symlinkJoin {
  name = "sherpa-onnx-link-dir";
  paths = [ sherpa-onnx onnxruntime ];
}
