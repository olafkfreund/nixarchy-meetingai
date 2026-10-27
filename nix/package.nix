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
  LIBCLANG_PATH = "${libclang.lib}/lib";

  # Tests open sockets and audio devices; they run in the dev shell.
  doCheck = false;

  postInstall = ''
    install -Dm644 packaging/dev.nixarchy.meetingai.desktop \
      $out/share/applications/dev.nixarchy.meetingai.desktop
    install -Dm644 src/assets/nixarchy-meetingai.png \
      $out/share/icons/hicolor/256x256/apps/dev.nixarchy.meetingai.png
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
