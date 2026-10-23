{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.lennettePrivateOverlay;
  launcher = pkgs.writeShellApplication {
    name = "lennette-private-user";
    runtimeInputs = [
      self.packages.${pkgs.stdenv.hostPlatform.system}.hyprland-private
      pkgs.jq
      pkgs.grim
      pkgs.pulseaudio
    ];
    text = ''
      export LENNETTE_BINARY=${lib.escapeShellArg cfg.executable}
      export LENNETTE_ALLOWED_PLUGIN_PATHS=${lib.escapeShellArg (builtins.toJSON cfg.allowedPluginPaths)}
      ${builtins.readFile ./launch.sh}
    '';
  };
  command = "${launcher}/bin/lennette-private-user";
in
{
  options.programs.lennettePrivateOverlay = {
    enable = lib.mkEnableOption "Lennette protected launcher and compositor keybindings";
    executable = lib.mkOption {
      type = lib.types.str;
      default = "${config.home.homeDirectory}/.local/bin/lennette";
      description = "Absolute path to the installed Lennette binary.";
    };
    allowedPluginPaths = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = "Exact shared-library paths of plugins rebuilt for the private compositor and verified with capture exclusion.";
    };
  };
  config = lib.mkIf cfg.enable {
    wayland.windowManager.hyprland = {
      enable = lib.mkDefault true;
      configType = "lua";
      package = lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.hyprland-private;
    };
    wayland.windowManager.hyprland.extraConfig = lib.mkAfter ''
      require('lennette')
    '';
    home.packages = [
      launcher
      pkgs.nerd-fonts.fira-code
    ];
    # The Nix-owned file also prevents Lennette's startup installer from replacing
    # these bindings with mutable ones that bypass the protected launcher.
    xdg.configFile."hypr/lennette.lua" = {
      force = true;
      text = ''
        ${builtins.readFile ./lennette-private.lua}
        hl.bind("CTRL + SHIFT + D", hl.dsp.exec_cmd("${command} --toggle-dashboard"))
        hl.bind("CTRL + SHIFT + code:51", hl.dsp.exec_cmd("${command} --toggle-window"))
        hl.bind("CTRL + SHIFT + I", hl.dsp.exec_cmd("${command} --focus-input"))
        hl.bind("CTRL + SHIFT + M", hl.dsp.exec_cmd("${command} --toggle-system-audio"))
        hl.bind("CTRL + SHIFT + A", hl.dsp.exec_cmd("${command} --toggle-voice"))
        hl.bind("CTRL + SHIFT + S", hl.dsp.exec_cmd("${command} --screenshot"))
      '';
    };
    xdg.desktopEntries.lennette-private = {
      name = "Lennette (private overlay)";
      comment = "An ambient real-time meeting and discussion copilot";
      exec = command;
      icon = "dev.lennette.overlay";
      terminal = false;
      categories = [ "Utility" ];
    };
    xdg.dataFile."icons/hicolor/512x512/apps/dev.lennette.overlay.png".source =
      ../../src/assets/lennette_app_icon.png;
  };
}
