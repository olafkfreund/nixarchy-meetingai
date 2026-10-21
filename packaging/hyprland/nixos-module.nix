{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.lennettePrivateOverlay;
in
{
  options.programs.lennettePrivateOverlay.enable = lib.mkEnableOption "Lennette capture exclusion in Hyprland";
  config = lib.mkIf cfg.enable {
    programs.hyprland.enable = lib.mkDefault true;
    programs.hyprland.package =
      lib.mkForce
        self.packages.${pkgs.stdenv.hostPlatform.system}.hyprland-private;
    environment.systemPackages = [ self.packages.${pkgs.stdenv.hostPlatform.system}.lennette-private ];
  };
}
