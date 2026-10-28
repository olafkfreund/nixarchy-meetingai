{
  lib,
  rustPlatform,
  pkg-config,
  perl,
  clang,
  libclang,
  alsa-lib,
  pipewire,
  openssl,
  makeWrapper,
  callPackage,
  grim,
  slurp,
  libnotify,
}:

rustPlatform.buildRustPackage {
  pname = "nixarchy-meetingai";
  version = (lib.importTOML ../Cargo.toml).package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      ../packaging
      ../tests/parakeet_decode.rs # run by the parakeet-decode flake check
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    perl
    clang
    makeWrapper
  ];
  buildInputs = [
    alsa-lib
    pipewire
    openssl
  ];
  # The sherpa-onnx crate would download its library at build time; use nixpkgs' instead.
  SHERPA_ONNX_LIB_DIR = "${callPackage ./sherpa-lib.nix { }}/lib";
  LIBCLANG_PATH = "${libclang.lib}/lib";

  # Tests open sockets and audio devices; they run in the dev shell.
  doCheck = false;

  postInstall = ''
    install -Dm644 packaging/dev.nixarchy.meetingai.desktop \
      $out/share/applications/dev.nixarchy.meetingai.desktop
    install -Dm644 packaging/nixarchy-meetingai.svg \
      $out/share/icons/hicolor/scalable/apps/dev.nixarchy.meetingai.svg
    wrapProgram $out/bin/nixarchy-meetingai \
      --prefix PATH : ${
        lib.makeBinPath [
          grim
          slurp
          libnotify
        ]
      }
  '';

  meta = {
    description = "Ambient real-time meeting copilot daemon for Omarchy";
    license = lib.licenses.agpl3Plus;
    mainProgram = "nixarchy-meetingai";
    platforms = lib.platforms.linux;
  };
}
