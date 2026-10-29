{
  description = "nixarchy-meetingai: ambient meeting copilot daemon and Quickshell plugin for Omarchy";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    systems.url = "github:nix-systems/default-linux";
    # Only so `nix flake check` can evaluate the module this repo ships.
    home-manager.url = "github:nix-community/home-manager";
    home-manager.inputs.nixpkgs.follows = "nixpkgs";
    # The call-detection plugin; `callDetection.enable` adds it.
    nixarchy-meeting-mode.url = "github:olafkfreund/nixarchy-meeting-mode";
    nixarchy-meeting-mode.inputs.nixpkgs.follows = "nixpkgs";
    # Only its packages.<sys>.omarchy: the shell tree (qs.Commons / qs.Ui) the QML tests run against.
    nixarchy.url = "github:olafkfreund/nixarchy";
    nixarchy.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      systems,
      home-manager,
      nixarchy-meeting-mode,
      nixarchy,
    }:
    let
      inherit (nixpkgs) lib;
      # The module's options as CommonMark, for the README section between the nix-options markers.
      optionsDoc =
        pkgs:
        let
          hm = home-manager.lib.homeManagerConfiguration {
            inherit pkgs;
            modules = [
              self.homeManagerModules.default
              {
                home = {
                  username = "docs";
                  homeDirectory = "/home/docs";
                  stateVersion = "25.05";
                };
              }
            ];
          };
          doc = pkgs.nixosOptionsDoc {
            options = {
              inherit (hm.options.programs) nixarchy-meetingai;
            };
            transformOptions = o: o // { declarations = [ ]; };
            warningsAreErrors = false;
          };
        in
        doc.optionsCommonMark;
      eachSystem = f: nixpkgs.lib.genAttrs (import systems) (system: f nixpkgs.legacyPackages.${system});
      # What the shell tests (shell/tests/*.sh) need on PATH and in the environment: the dev shell and the check share it.
      shellTools =
        pkgs: with pkgs; [
          bash
          coreutils
          findutils
          gnugrep
          gawk
          curl
          fontconfig # fc-match: the shell asks it for the monospace family
          dbus
          quickshell
          sway
          grim
          (python3.withPackages (ps: [ ps.pillow ]))
        ];
      shellTestEnv = pkgs: {
        OMARCHY_SHELL_TREE = "${
          nixarchy.packages.${pkgs.stdenv.hostPlatform.system}.omarchy
        }/share/omarchy/shell";
        QMLLINT = "${pkgs.qt6.qtdeclarative}/bin/qmllint";
        QT_QML = "${pkgs.qt6.qtdeclarative}";
        QUICKSHELL_QML = "${pkgs.quickshell}/lib/qt-6/qml";
        SWAY_BIN = "${pkgs.sway}/bin/sway";
        FONTCONFIG_FILE = pkgs.makeFontsConf { fontDirectories = [ pkgs.nerd-fonts.jetbrains-mono ]; };
        SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"; # the daemon panics without a CA store
        LANG = "C.UTF-8"; # Qt wants a UTF-8 locale; the build sandbox has none
        QT_QPA_PLATFORM = "wayland";
        QT_QUICK_BACKEND = "software"; # no GPU in a CI sandbox
      };
    in
    {
      packages = eachSystem (
        pkgs: {
          default = self.packages.${pkgs.stdenv.hostPlatform.system}.nixarchy-meetingai;
          nixarchy-meetingai = pkgs.callPackage ./nix/package.nix { };
          plugin = pkgs.callPackage ./nix/plugin.nix { };
          docs = optionsDoc pkgs;
        }
      );

      devShells = eachSystem (pkgs: {
        # The devenv.nix toolchain without devenv, so CI needs only Nix.
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            git
            pkg-config
            perl # the vendored sqlcipher/openssl build
            gnumake
            llvmPackages.clang
            openssl
            pipewire
            alsa-lib
            cacert
          ];
          # nix-shell's TMPDIR is too long for tests/daemon_flow.rs: unix socket paths cap at 108 bytes.
          shellHook = "export TMPDIR=/tmp";
          env = rec {
            RUST_BACKTRACE = "1";
            LD_LIBRARY_PATH = lib.makeLibraryPath (import ./packaging/runtime-libraries.nix { inherit pkgs; });
            NIXARCHY_MEETINGAI_RUNTIME_LIBRARY_PATH = LD_LIBRARY_PATH;
            CPATH = "${pkgs.glibc.dev}/include";
            BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.glibc.dev}/include";
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            PKG_CONFIG_PATH = "${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.pipewire.dev}/lib/pkgconfig";
            SHERPA_ONNX_LIB_DIR = "${pkgs.callPackage ./nix/sherpa-lib.nix { }}/lib";
            SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
            NIX_SSL_CERT_FILE = SSL_CERT_FILE;
          };
        };
        # `nix develop .#shell --command bash shell/tests/lint.sh` (and the other shell tests).
        shell = pkgs.mkShell {
          packages = shellTools pkgs;
          env = shellTestEnv pkgs;
        };
      });

      homeManagerModules = rec {
        default = import ./nix/hm-module.nix self { inherit nixarchy-meeting-mode; };
        nixarchy-meetingai = default;
      };

      overlays.default = final: _prev: {
        nixarchy-meetingai = final.callPackage ./nix/package.nix { };
        nixarchy-meetingai-plugin = final.callPackage ./nix/plugin.nix { };
      };

      # A module nothing evaluates is a module nothing checks.
      checks = eachSystem (
        pkgs:
        let
          evalHm =
            extra:
            (home-manager.lib.homeManagerConfiguration {
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
                extra
              ];
            }).config;
          minimal = evalHm { };
          full = evalHm {
            programs.nixarchy-meetingai = {
              autostart = false;
              logLevel = "debug";
              extraEnvironment.FOO = "bar";
              ollamaCloud.keyFile = "/run/agenix/ollama-cloud";
              deepgram.keyFile = "/run/agenix/deepgram";
              deepgram.region = "eu";
              ollama.url = "http://127.0.0.1:12345";
              settings = {
                enforce = true;
                meetingMode = "interviewer";
                chatProvider = "claude-sub";
                chatModel = "opus";
                liveProvider = "claude-sub";
                liveModel = "haiku";
                responsePace = "Fast";
                responseLength = "Short";
                responseLanguage = "English";
                autoResponse = "pauses";
                smartFollowUps = false;
                autoRecordOnCall = true;
                calendarEnabled = true;
                speechProvider = "parakeet-local";
                speechModel = "parakeet-tdt-0.6b-v3";
                speechLanguage = "en";
                textScale = 120;
                reduceMotion = true;
                mcpAllowRecord = false;
              };
            };
          };
          # Stand-in for nixarchy's own option, whose module is not an input of this flake.
          stubPlugins = {
            options.programs.nixarchy.plugins = nixpkgs.lib.mkOption {
              type = lib.types.attrsOf (
                lib.types.submodule { options.src = nixpkgs.lib.mkOption { type = lib.types.path; }; }
              );
              default = { };
            };
          };
          withPlugins = evalHm stubPlugins;
          noSkills = evalHm { programs.nixarchy-meetingai.skills.enable = false; };
          userMeetingMode = evalHm {
            imports = [ stubPlugins ];
            programs.nixarchy.plugins."nixarchy.meeting-mode".src = "/user/meeting-mode";
          };
          echoOff = evalHm { programs.nixarchy-meetingai.echoCancel.enable = false; };
          echoFile = "pipewire/pipewire.conf.d/60-meetingai-echo-cancel.conf";
          svc = c: c.systemd.user.services;
          assertThat =
            name: cond: msg:
            pkgs.writeText name (if cond then "ok" else throw "${name}: ${msg}");
          fullDaemon = (svc full).nixarchy-meetingai.Service;
          sysPkgs = self.packages.${pkgs.stdenv.hostPlatform.system};
          noParakeet = evalHm {
            programs.nixarchy-meetingai = {
              parakeet.enable = false;
              settings.speechProvider = "deepgram-stt";
            };
          };
        in
        {
          # The QML suite (lint, smoke, keys, every live-shots scenario with geometry checks, contrast) in a headless sway.
          shell =
            pkgs.runCommand "shell-check"
              (
                shellTestEnv pkgs
                // {
                  nativeBuildInputs = shellTools pkgs;
                  MEETINGAI_BIN = "${sysPkgs.default}/bin/nixarchy-meetingai";
                }
              )
              ''
                cp -r ${
                  lib.fileset.toSource {
                    root = ./shell;
                    fileset = ./shell;
                  }
                } shell
                chmod -R u+w shell
                bash shell/tests/ci.sh $out
              '';
          # The package's decode test against the real model and a clip shipped in its archive: a
          # nixpkgs bump of sherpa-onnx/onnxruntime that breaks the crate (2.0.1 vs library 1.13.x) fails here.
          parakeet-decode =
            let
              model = pkgs.callPackage ./nix/parakeet-model.nix { };
            in
            sysPkgs.default.overrideAttrs (_: {
              doCheck = true;
              cargoTestFlags = [
                "--test"
                "parakeet_decode"
              ];
              NIXARCHY_MEETINGAI_PARAKEET_DIR = "${model}";
              NIXARCHY_MEETINGAI_PARAKEET_WAV = "${model}/test_wavs/en.wav";
            });
          # The sherpa-onnx crate and the C library it links must be the same version (pure eval).
          sherpa-version-match =
            let
              crate = (lib.findFirst (p: p.name == "sherpa-onnx") null (builtins.fromTOML (builtins.readFile ./Cargo.lock)).package).version;
              guard =
                crateVersion:
                assertThat "sherpa-version-match" (crateVersion == (pkgs.callPackage ./nix/sherpa-onnx.nix { }).version)
                  "Cargo.lock sherpa-onnx ${crateVersion} != library ${(pkgs.callPackage ./nix/sherpa-onnx.nix { }).version}";
            in
            # The guard must also fail on a mismatch, else it proves nothing.
            assert !(builtins.tryEval (builtins.readFile (guard "0.0.0"))).success;
            guard crate;
          # So `nix flake check` builds them.
          package = sysPkgs.default;
          plugin = sysPkgs.plugin;
          hm-module = pkgs.writeText "hm-module-check" (builtins.toJSON (builtins.attrNames (svc minimal)));
          hm-skills = assertThat "hm-skills" (
            builtins.all (p: minimal.home.file ? ${p} && !(noSkills.home.file ? ${p})) [
              ".claude/skills/meetingai"
              ".agents/skills/meetingai"
              ".gemini/antigravity/skills/meetingai"
            ]
          ) "skills.enable must link all three agent skill directories, and skills.enable = false none";
          hm-echo-cancel = assertThat "hm-echo-cancel" (
            minimal.xdg.configFile ? ${echoFile}
            && !(echoOff.xdg.configFile ? ${echoFile})
            && lib.hasInfix "meetingai-echo-cancel-source" minimal.xdg.configFile.${echoFile}.text
            && lib.hasInfix "flags = [ ifexists nofail ]" minimal.xdg.configFile.${echoFile}.text
          ) "echo-cancel conf must exist with echoCancel on and be absent when off";
          hm-plugin-nixarchy = assertThat "hm-plugin-nixarchy" (
            withPlugins.programs.nixarchy.plugins."nixarchy.meetingai".src
            == self.packages.${pkgs.stdenv.hostPlatform.system}.plugin
            &&
              withPlugins.programs.nixarchy.plugins."nixarchy.meeting-mode".src
              == nixarchy-meeting-mode.packages.${pkgs.stdenv.hostPlatform.system}.default
            && !(withPlugins.xdg.configFile ? "omarchy/plugins/nixarchy.meetingai")
            && userMeetingMode.programs.nixarchy.plugins."nixarchy.meeting-mode".src == "/user/meeting-mode"
            &&
              builtins.attrNames userMeetingMode.programs.nixarchy.plugins == [
                "nixarchy.meeting-mode"
                "nixarchy.meetingai"
              ]
          ) "plugins not registered through programs.nixarchy.plugins";
          hm-plugin-xdg = assertThat "hm-plugin-xdg" (
            minimal.xdg.configFile ? "omarchy/plugins/nixarchy.meetingai" && !(minimal.programs ? nixarchy)
          ) "plugin not installed to the omarchy folder without nixarchy";
          # Home Manager throws "Failed assertions" on any access to a config that breaks one.
          hm-assertions =
            let
              evaluates = extra: (builtins.tryEval (evalHm extra).home.username).success;
              wrong = [
                { programs.nixarchy-meetingai.whisper.enable = false; }
                { programs.nixarchy-meetingai.settings.speechProvider = "whisper-local"; }
                { programs.nixarchy-meetingai.parakeet.enable = false; }
                {
                  programs.nixarchy-meetingai = {
                    parakeet.enable = false;
                    settings.speechProvider = "parakeet-local";
                  };
                }
                { programs.nixarchy-meetingai.settings.extra.api_key = "fake-key"; }
                { programs.nixarchy-meetingai.ollama.url = "http://10.0.0.5:11434"; }
                { programs.nixarchy-meetingai.settings.speechBaseUrl = "http://10.0.0.5:8000"; }
                {
                  programs.nixarchy-meetingai.ollamaCloud = {
                    keyFile = "/run/fake";
                    environmentFile = "/run/fake.env";
                  };
                }
              ];
            in
            assertThat "hm-assertions" (
              evaluates { }
              && lib.all (c: !evaluates c) wrong
              && evaluates { programs.nixarchy-meetingai.ollama.url = "https://ollama.example.com"; }
              && evaluates { programs.nixarchy-meetingai.ollama.url = "http://[::1]:11434"; }
              && evaluates { programs.nixarchy-meetingai.settings.speechBaseUrl = "http://127.0.0.1:8000/v1"; }
              && evaluates {
                programs.nixarchy-meetingai.settings.speechBaseUrl = "https://speech.example.com/v1";
              }
              && evaluates {
                programs.nixarchy-meetingai = {
                  parakeet.enable = false;
                  settings.speechProvider = "deepgram-stt";
                };
              }
            ) "an assertion did not fire (or fired for a valid setup)";
          docs-match =
            let
              readme = builtins.readFile ./README.md;
              afterBegin = builtins.elemAt (lib.splitString "<!-- nix-options:begin -->\n" readme) 1;
              committed = builtins.head (lib.splitString "<!-- nix-options:end -->" afterBegin);
            in
            pkgs.runCommandLocal "docs-match" { } ''
              printf '%s' ${lib.escapeShellArg committed} > committed.md
              if ! diff -u committed.md ${self.packages.${pkgs.stdenv.hostPlatform.system}.docs}; then
                echo "README 'Nix options' is stale: nix build .#docs and paste result between the markers" >&2
                exit 1
              fi
              touch $out
            '';
          hm-settings-file = assertThat "hm-settings-file" (
            builtins.fromJSON full.xdg.configFile."nixarchy-meetingai/settings.json".text
            == builtins.fromJSON (builtins.readFile ./tests/fixtures/nix-settings.json)
            && !(minimal.xdg.configFile ? "nixarchy-meetingai/settings.json")
          ) "settings.json differs from tests/fixtures/nix-settings.json";
          hm-service-options = assertThat "hm-service-options" (
            builtins.elem "NIXARCHY_MEETINGAI_PARAKEET_DIR=${full.programs.nixarchy-meetingai.parakeet.model}" fullDaemon.Environment
            && builtins.elem "RUST_LOG=nixarchy_meetingai=debug" fullDaemon.Environment
            && builtins.elem "FOO=bar" fullDaemon.Environment
            && builtins.elem "NIXARCHY_MEETINGAI_OLLAMA_KEY_FILE=/run/agenix/ollama-cloud" fullDaemon.Environment
            && builtins.elem "NIXARCHY_MEETINGAI_DEEPGRAM_KEY_FILE=/run/agenix/deepgram" fullDaemon.Environment
            && !(lib.hasInfix "DEEPGRAM_KEY=" (toString fullDaemon.Environment))
            && !(fullDaemon ? LoadCredential)
            && !(builtins.any (lib.hasPrefix "NIXARCHY_MEETINGAI_PARAKEET_DIR=") (
              (svc noParakeet).nixarchy-meetingai.Service.Environment
            ))
            && !(svc full ? nixarchy-meetingai-whisper)
            && builtins.length (svc full).nixarchy-meetingai.Unit.X-Restart-Triggers == 2
            && builtins.elem "NIXARCHY_MEETINGAI_OLLAMA_URL=http://127.0.0.1:12345" fullDaemon.Environment
            && (svc full).nixarchy-meetingai.Install.WantedBy == [ ]
          ) "service options not applied";
        }
      );
    };
}
