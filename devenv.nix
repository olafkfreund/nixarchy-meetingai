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
    LD_LIBRARY_PATH = runtimeLibraryPath;
    # Embed the same paths at installation; desktop launchers have no dev shell.
    LENNETTE_RUNTIME_LIBRARY_PATH = runtimeLibraryPath;
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
    patchelf
    llvmPackages.clang
    llvmPackages.libclang
    openssl
    # gpui native dependency surface
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
    grim
    wlrctl
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
  # libspa-sys generates bindings with bindgen, which needs the C standard
  # headers on clang's search path.
  env.CPATH = "${pkgs.glibc.dev}/include";

  env.BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.glibc.dev}/include";

  env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

  env.PKG_CONFIG_PATH = "${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.pipewire.dev}/lib/pkgconfig:${pkgs.libGL.dev}/lib/pkgconfig";

  env.SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  env.NIX_SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  env.XDG_DATA_DIRS = "${pkgs.fontconfig}/share:$XDG_DATA_DIRS";

  git-hooks.hooks = {
    nixfmt = {
      enable = true;
      package = pkgs.nixfmt;
    };
    rustfmt.enable = true;
    # The pre-commit hook runs outside the shell env with the user's PATH, which
    # can contain a newer nightly rustc. Build the PATH explicitly instead of
    # appending to it, so the pinned toolchain always wins.
    clippy = {
      enable = true;
      entry = "${pkgs.writeShellScript "lennette-clippy" ''
        # Use the complete toolchain (rustc + std + clippy together). Splitting
        # them across separate outputs makes clippy-driver unable to find std.
        export PATH="${
          lib.makeBinPath [
            pkgs.rust-bin.stable."1.99.1".default
            pkgs.pkg-config
            pkgs.coreutils
            pkgs.gcc
            pkgs.glibc.bin
            pkgs.llvmPackages.clang
          ]
        }"
        export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
        export BINDGEN_EXTRA_CLANG_ARGS="-isystem ${pkgs.glibc.dev}/include"
        export CPATH="${pkgs.glibc.dev}/include"
        export PKG_CONFIG_PATH="${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.pipewire.dev}/lib/pkgconfig:${pkgs.libGL.dev}/lib/pkgconfig:${pkgs.mesa}/lib/pkgconfig"
        cd "''${DEVENV_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
        exec cargo clippy --all-targets --all-features -- -D warnings
      ''}";
      pass_filenames = false;
    };
  };

  tasks = {
    "lennette:build" = {
      exec = "cargo build";
    };
    "lennette:test" = {
      exec = "cargo nextest run";
    };
    "lennette:check" = {
      exec = "cargo fmt --check && cargo clippy --all-targets -- -D warnings";
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
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo nextest run
  '';
}
