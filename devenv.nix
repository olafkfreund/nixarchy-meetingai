{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

{
  # https://devenv.sh/reference/options/
  name = "lennette";
  cachix.enable = true;
  dotenv.enable = true;
  dotenv.disableHint = true;

  # Pin the exact current stable toolchain from rust-overlay.
  overlays = [ (import inputs.rust-overlay) ];

  env = {
    RUST_BACKTRACE = "1";
    # wayland/x11 + mesa for gpui's blade/wgpu renderer
    LD_LIBRARY_PATH = lib.makeLibraryPath [
      pkgs.vulkan-loader
      pkgs.libGL
      pkgs.wayland
      pkgs.libxkbcommon
      pkgs.libx11
      pkgs.libxcursor
      pkgs.libxrandr
      pkgs.libxi
      pkgs.pipewire
      pkgs.alsa-lib
      pkgs.openssl
      pkgs.stdenv.cc.cc.lib
    ];
  };

  packages = with pkgs; [
    git
    curl
    jq
    just
    rustfmt
    clippy
    cargo-nextest
    cargo-deny
    pkg-config
    openssl
    # gpui native dependency surface
    vulkan-loader
    vulkan-headers
    mesa
    wayland
    wayland-protocols
    libxkbcommon
    libx11
    libxcursor
    libxrandr
    libxi
    fontconfig
    freetype
    # audio + capture
    pipewire
    alsa-lib
    alsa-lib.dev
    pulseaudio
    libpulseaudio
    xorg.xauth
    xorg.xvfb
    imagemagick
    # secure storage / http
    sqlite
    zlib
    cacert
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
    # Pin the exact current stable toolchain (1.99.1) from rust-overlay rather
    # than following whatever nixpkgs happens to ship.
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

  # CPAL links against ALSA; pkg-config must find its headers or `cargo clippy`
  # fails inside git hooks where the shell env is not inherited.
  env.PKG_CONFIG_PATH = "${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.pipewire.dev}/lib/pkgconfig";

  env.SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  env.NIX_SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  env.XDG_DATA_DIRS = "${pkgs.fontconfig}/share:$XDG_DATA_DIRS";

  git-hooks.hooks = {
    nixfmt = {
      enable = true;
      package = pkgs.nixfmt;
    };
    rustfmt.enable = true;
    # The pre-commit hook runs outside the shell env, so it carries its own
    # toolchain and the pkg-config paths CPAL's ALSA dependency needs.
    clippy = {
      enable = true;
      entry = "${pkgs.writeShellScript "lennette-clippy" ''
        export PATH="${
          lib.makeBinPath [
            pkgs.rust-bin.stable."1.99.1".cargo
            pkgs.rust-bin.stable."1.99.1".rustc
            pkgs.pkg-config
          ]
        }:$PATH"
        export PKG_CONFIG_PATH="${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.pipewire.dev}/lib/pkgconfig"
        exec cargo clippy --all-targets --all-features -- -D warnings
      ''}";
      pass_filenames = false;
    };
  };

  tasks = {
    "lennette:build" = {
      exec = "cargo build --workspace";
    };
    "lennette:test" = {
      exec = "cargo nextest run --workspace";
    };
    "lennette:check" = {
      exec = "cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings";
    };
    "lennette:run" = {
      exec = "cargo run -p lennette";
    };
  };

  processes = { };

  enterShell = ''
    echo "◆ lennette devenv — NixOS"
    echo "  rust $(rustc --version) | cargo $(cargo --version)"
    echo "  tip: just build | just test | just run"
  '';

  enterTest = ''
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo nextest run --workspace
  '';
}
