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
  };
  config = lib.mkIf cfg.enable {
    wayland.windowManager.hyprland = {
      enable = lib.mkDefault true;
      configType = "lua";
      package = lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.hyprland-private;
    };
    # Overview/render plugins can copy display snapshots back into a clean render.
    # They also target a different Hyprland ABI. Audit before enabling any plugin.
    wayland.windowManager.hyprland.plugins = lib.mkForce [ ];
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
    # Route the ordinary app-menu entry through the same protected launcher.
    xdg.dataFile."applications/dev.lennette.overlay.desktop" = {
      force = true;
      text = ''
        [Desktop Entry]
        Type=Application
        Name=Lennette
        Comment=An ambient real-time meeting and discussion copilot
        Exec=${command}
        Icon=dev.lennette.overlay
        Terminal=false
        Categories=Utility;
        StartupWMClass=dev.lennette.overlay
      '';
    };
  };
}
