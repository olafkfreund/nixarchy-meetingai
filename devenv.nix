{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

let
  runtimeLibraryPath = lib.makeLibraryPath (
    import ./packaging/runtime-libraries.nix { inherit pkgs; }
  );
in
{
  name = "lennette";
  cachix.enable = true;
  dotenv.enable = true;
  dotenv.disableHint = true;
  overlays = [ (import inputs.rust-overlay) ];

  env = {
    RUST_BACKTRACE = "1";
    LD_LIBRARY_PATH = runtimeLibraryPath;
    LENNETTE_RUNTIME_LIBRARY_PATH = runtimeLibraryPath;
  };

  packages = with pkgs; [
    git
    curl
    jq
    just
    rustfmt
    lefthook
    clippy
    cargo-nextest
    cargo-deny
    pkg-config
    patchelf
    llvmPackages.clang
    llvmPackages.libclang
    openssl
    vulkan-loader
    vulkan-headers
    mesa
    libgbm
    libGL.dev
    libGLU
    wayland
    wayland-protocols
    libxkbcommon
    libx11
    libxcb
    libxcursor
    libxrandr
    libxi
    fontconfig
    freetype
    pipewire
    alsa-lib
    alsa-lib.dev
    pulseaudio
    libpulseaudio
    xorg.xauth
    xorg.xvfb
    imagemagick
    grim
    wlrctl
    sqlite
    zlib
    cacert
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
    toolchain = {
      rustc = pkgs.rust-bin.stable."1.99.1".rustc;
      cargo = pkgs.rust-bin.stable."1.99.1".cargo;
      clippy = pkgs.rust-bin.stable."1.99.1".clippy;
      rustfmt = pkgs.rust-bin.stable."1.99.1".rustfmt;
      rust-analyzer = pkgs.rust-bin.stable."1.99.1".rust-analyzer;
    };
    components = [
      "rustc"
      "cargo"
      "clippy"
      "rustfmt"
      "rust-analyzer"
    ];
  };

  env.CPATH = "${pkgs.glibc.dev}/include";

  env.BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.glibc.dev}/include";

  env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

  env.PKG_CONFIG_PATH = "${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.pipewire.dev}/lib/pkgconfig:${pkgs.libGL.dev}/lib/pkgconfig";

  env.SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  env.NIX_SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  env.XDG_DATA_DIRS = "${pkgs.fontconfig}/share:$XDG_DATA_DIRS";

  tasks = {
    "lennette:build" = {
      exec = "cargo build";
    };
    "lennette:test" = {
      exec = "cargo nextest run --workspace";
    };
    "lennette:check" = {
      exec = "cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings";
    };
    "lennette:run" = {
      exec = "cargo run";
    };
  };

  processes = { };

  enterShell = ''
    echo "◆ lennette devenv — NixOS"
    echo "  rust $(rustc --version) | cargo $(cargo --version)"
    echo "  tip: just build | just test | just run"
  '';

  enterTest = ''
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo nextest run --workspace
  '';
}
