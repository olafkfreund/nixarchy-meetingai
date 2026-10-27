{ pkgs }:
with pkgs;
[
  pipewire
  alsa-lib
  openssl
  stdenv.cc.cc.lib
]
