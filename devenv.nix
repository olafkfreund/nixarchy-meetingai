{ pkgs, inputs, ... }:
let
  toolchain = pkgs.rust-bin.stable."1.99.1".default.override {
    extensions = [ "rust-src" "clippy" "rustfmt" "rust-analyzer" ];
  };
  sherpaLib = pkgs.callPackage ./nix/sherpa-lib.nix { };
  certBundle = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
in
{
  name = "nixarchy-meetingai";

  overlays = [ (import inputs.rust-overlay) ];

  languages.rust = {
    enable = true;
    toolchainPackage = toolchain;
  };

  packages = with pkgs; [
    git
    pkg-config
    clang
    llvmPackages.libclang
    openssl
    pipewire
    alsa-lib
    alsa-lib.dev
    cacert
    perl # the vendored sqlcipher/openssl build
    gnumake
    grim # the daemon shells out to grim and slurp for screenshots
    slurp
    jq
    curl
    sqlite
  ];

  env = {
    RUST_BACKTRACE = "1";
    # unix socket paths in tests/daemon_flow.rs cap at 108 bytes
    TMPDIR = "/tmp";
    LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (import ./packaging/runtime-libraries.nix { inherit pkgs; });
    CPATH = "${pkgs.glibc.dev}/include";
    BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.glibc.dev}/include";
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
    PKG_CONFIG_PATH = "${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.pipewire.dev}/lib/pkgconfig";
    SHERPA_ONNX_LIB_DIR = "${sherpaLib}/lib";
    SSL_CERT_FILE = certBundle;
    NIX_SSL_CERT_FILE = certBundle;
  };

  dotenv = {
    enable = true;
    disableHint = true;
  };

  enterShell = ''
    echo "nixarchy-meetingai dev shell: cargo build, cargo test --workspace"
  '';

  # What CI runs.
  enterTest = ''
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace --no-fail-fast
  '';
}
