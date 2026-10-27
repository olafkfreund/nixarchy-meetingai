{
  description = "nixarchy-meetingai: ambient meeting copilot daemon and Quickshell plugin for Omarchy";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    systems.url = "github:nix-systems/default-linux";
    # Only so `nix flake check` can evaluate the module this repo ships.
    home-manager.url = "github:nix-community/home-manager";
    home-manager.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      systems,
      home-manager,
    }:
    let
      eachSystem = f: nixpkgs.lib.genAttrs (import systems) (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = eachSystem (
        pkgs:
        let
          models = pkgs.callPackage ./nix/whisper-model.nix { };
        in
        {
          default = pkgs.callPackage ./nix/package.nix { };
          whisper-model-small-en = models."small.en";
          whisper-model-small = models."small";
        }
      );

      homeManagerModules.default = import ./nix/hm-module.nix self;

      overlays.default = final: _prev: {
        nixarchy-meetingai = final.callPackage ./nix/package.nix { };
      };

      # A module nothing evaluates is a module nothing checks.
      checks = eachSystem (pkgs: {
        hm-module =
          let
            hm = home-manager.lib.homeManagerConfiguration {
              inherit pkgs;
              modules = [
                self.homeManagerModules.default
                {
                  home = {
                    username = "check";
                    homeDirectory = "/home/check";
                    stateVersion = "25.05";
                  };
                  programs.nixarchy-meetingai.enable = true;
                }
              ];
            };
          in
          pkgs.writeText "hm-module-check" (
            builtins.toJSON (builtins.attrNames hm.config.systemd.user.services)
          );
      });
    };
}
