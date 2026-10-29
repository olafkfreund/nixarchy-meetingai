  name = "nixarchy-meetingai";
    NIXARCHY_MEETINGAI_RUNTIME_LIBRARY_PATH = runtimeLibraryPath;
  env.PKG_CONFIG_PATH = "${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.pipewire.dev}/lib/pkgconfig";
  env.SHERPA_ONNX_LIB_DIR = "${pkgs.callPackage ./nix/sherpa-lib.nix { }}/lib";

    "nixarchy-meetingai:build" = {
    "nixarchy-meetingai:test" = {
    "nixarchy-meetingai:check" = {
    "nixarchy-meetingai:run" = {
    echo "◆ nixarchy-meetingai devenv — NixOS"
