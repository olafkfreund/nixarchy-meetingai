{ pkgs }:
with pkgs;
[
  vulkan-loader
  libGL
  libgbm
  wayland
  libxkbcommon
  libxcb
  libx11
  libxcursor
  libxrandr
  libxi
  fontconfig
  freetype
  pipewire
  alsa-lib
  libpulseaudio
  openssl
  stdenv.cc.cc.lib
]
