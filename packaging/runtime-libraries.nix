# Shared libraries the binary dlopens or links at run time.
{ pkgs }:
[
  pkgs.pipewire
  pkgs.alsa-lib
  pkgs.openssl
  pkgs.stdenv.cc.cc.lib
]
