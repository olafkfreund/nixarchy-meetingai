{
  description = "Lennette private overlays for a pinned Hyprland compositor";

  inputs.hyprland.url = "github:hyprwm/Hyprland/c7216a9807a2683ce243f6ed14ae01e7e884c426";
  inputs.nixpkgs.follows = "hyprland/nixpkgs";

  outputs =
    {
      self,
      hyprland,
      nixpkgs,
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      compositor = hyprland.packages.${system}.hyprland.overrideAttrs (old: {
        patches = (old.patches or [ ]) ++ [
          ./private-overlay.patch
          ./plugin-compat.patch
        ];
      });
    in
    {
      # Build plugins with the compositor's compiler and dependency closure.
      legacyPackages.${system} = import nixpkgs {
        inherit system;
        overlays = [
          hyprland.overlays.hyprland-packages
          (_final: _prev: { hyprland = compositor; })
        ];
      };
      packages.${system} = {
        default = compositor;
        hyprland-private = compositor;
        lennette-private = pkgs.writeShellApplication {
          name = "lennette-private";
          runtimeInputs = [
            compositor
            pkgs.jq
            pkgs.grim
            pkgs.pulseaudio
          ];
          text = builtins.readFile ./launch.sh;
        };
      };
      devShells.${system}.default = hyprland.devShells.${system}.default.overrideAttrs (old: {
        # Upstream's hook installs Git hooks into the caller's repository.
        shellHook = ''
          export HYPRLAND_SOURCE=${hyprland.packages.${system}.hyprland.src}
        '';
        nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [
          pkgs.cargo
          pkgs.rustc
          pkgs.grim
          pkgs.jq
          pkgs.wlrctl
        ];
      });
      nixosModules.default = import ./nixos-module.nix { inherit self; };
      homeManagerModules.default = import ./home-module.nix { inherit self; };
      checks.${system} = {
        patch = pkgs.runCommand "lennette-private-patch-check" { nativeBuildInputs = [ pkgs.git ]; } ''
          cp -r ${hyprland.packages.${system}.hyprland.src} source
          chmod -R u+w source
          cd source
          git apply --check ${./private-overlay.patch}
          git apply ${./private-overlay.patch}
          git apply --check ${./plugin-compat.patch}
          touch "$out"
        '';
        launcher =
          pkgs.runCommand "lennette-private-launcher-check"
            {
              nativeBuildInputs = [
                pkgs.bash
                pkgs.shellcheck
                pkgs.jq
              ];
            }
            ''
              bash -n ${./launch.sh}
              shellcheck ${./launch.sh}
              bash ${./.}/tests/test-launcher.sh
              touch "$out"
            '';
      };
    };
}
