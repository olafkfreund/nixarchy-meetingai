# nixpkgs' sherpa-onnx (1.13.8) raised to 2.0.1 so the C library matches the sherpa-onnx crate in Cargo.lock.
# Drop this when nixpkgs ships 2.0.x; the `sherpa-version-match` flake check fails on any drift.
{ sherpa-onnx, fetchFromGitHub, fetchurl }:
let
  version = "2.0.1";
  fbank = fetchurl {
    url = "https://github.com/csukuangfj/kaldi-native-fbank/archive/refs/tags/v1.22.4.tar.gz";
    hash = "sha256-JX/qY5WFym7gCGBJO7nXqzJIi4pUB//kNMHzYepUX64=";
  };
  utf8proc = fetchurl {
    url = "https://github.com/JuliaStrings/utf8proc/archive/refs/tags/v2.12.0.tar.gz";
    hash = "sha256-9WQBHTiyiI1YPVELCOaf+hWqEXFV2xubSe8d/h+iURE=";
  };
in
sherpa-onnx.overrideAttrs (old: {
  inherit version;
  src = fetchFromGitHub {
    owner = "k2-fsa";
    repo = "sherpa-onnx";
    tag = "v${version}";
    hash = "sha256-rcs6RFyQujOvm3pNuA8AxzwEluF5/Ei1XQowYaq+bGc=";
  };
  # 2.0.1 dropped the espeak-ng-for-piper cmake file this patch edits.
  patches = [ ];
  # The old cache list stays (unused files are harmless); 2.0.1 needs these two on top.
  preConfigure = old.preConfigure + ''
    cp ${fbank} ./kaldi-native-fbank-1.22.4.tar.gz
    cp ${utf8proc} ./utf8proc-2.12.0.tar.gz
  '';
})
