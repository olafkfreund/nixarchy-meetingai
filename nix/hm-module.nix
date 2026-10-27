self: inputs:
{
  config,
  lib,
  options,
  pkgs,
  ...
}:

let
  cfg = config.programs.nixarchy-meetingai;
  sys = pkgs.stdenv.hostPlatform.system;
  modelFile = "${cfg.whisper.model}/ggml-${cfg.whisper.model.modelName}.bin";
  bin = lib.getExe cfg.package;
  whisperAddr = "${cfg.whisper.host}:${toString cfg.whisper.port}";

  # nixarchy (when imported) installs plugins; without it the plugin goes to omarchy's folder.
  hasPlugins = lib.hasAttrByPath [ "programs" "nixarchy" "plugins" ] options;
  plugin = self.packages.${sys}.plugin;
  callDetectionPlugin = inputs.nixarchy-meeting-mode.packages.${sys}.default;

  # Allowed values for the enum settings, checked against the Rust types by a unit test.
  enums = builtins.fromJSON (builtins.readFile ./settings-enums.json);
  nullable =
    type: description:
    lib.mkOption {
      type = lib.types.nullOr type;
      default = null;
      inherit description;
    };
  str = lib.types.str;
  bool = lib.types.bool;

  # setting key -> value; the app stores strings
  typedSettings = with cfg.settings; {
    meeting_mode = meetingMode;
    chat_provider = chatProvider;
    chat_model = chatModel;
    live_provider = liveProvider;
    live_model = liveModel;
    response_pace = responsePace;
    response_length = responseLength;
    response_language = responseLanguage;
    auto_response = autoResponse;
    smart_follow_ups = smartFollowUps;
    auto_record_on_call = autoRecordOnCall;
    speech_provider = speechProvider;
    speech_model = speechModel;
    speech_language = speechLanguage;
    text_scale = textScale;
    reduce_motion = reduceMotion;
    mcp_allow_record = mcpAllowRecord;
  };
  settingValues =
    cfg.settings.extra
    // lib.mapAttrs (_: v: if builtins.isBool v then lib.boolToString v else toString v) (
      lib.filterAttrs (_: v: v != null) typedSettings
    );
  settingsFile = builtins.toJSON {
    inherit (cfg.settings) enforce;
    values = settingValues;
  };

  # option name -> [ description command ]
  keys = {
    overlay = [
      "Meeting AI overlay"
      "omarchy-shell shell toggle nixarchy.meetingai '{}'"
    ];
    record = [
      "Meeting AI: start recording"
      "${bin} ctl record.start"
    ];
    stop = [
      "Meeting AI: stop recording"
      "${bin} ctl record.stop"
    ];
    mic = [
      "Meeting AI: toggle microphone"
      "${bin} ctl record.toggle_mic"
    ];
    system = [
      "Meeting AI: toggle system audio"
      "${bin} ctl record.toggle_system"
    ];
    screenshot = [
      "Meeting AI: screenshot"
      "${bin} ctl screenshot.capture '{\"region\":true}'"
    ];
  };

  bindLine =
    name: key:
    let
      spec = keys.${name};
    in
    "  o.bind(${builtins.toJSON key}, ${builtins.toJSON (builtins.elemAt spec 0)}, ${builtins.toJSON (builtins.elemAt spec 1)})";
in
{
  options.programs.nixarchy-meetingai = {
    enable = lib.mkEnableOption "the nixarchy-meetingai daemon, its Quickshell plugin and local speech-to-text";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${sys}.default;
      defaultText = lib.literalExpression "nixarchy-meetingai.packages.\${system}.default";
      description = "The nixarchy-meetingai package.";
    };

    autostart = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Start the services with the graphical session.";
    };

    logLevel = lib.mkOption {
      type = lib.types.enum [
        "error"
        "warn"
        "info"
        "debug"
        "trace"
      ];
      default = "info";
      description = "Daemon log level (`RUST_LOG` for the daemon's own crate).";
    };

    extraEnvironment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      example = {
        HTTPS_PROXY = "http://proxy:3128";
      };
      description = "Extra environment for the daemon service. Never put secrets here: it lands in the Nix store.";
    };

    whisper = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Run the local whisper.cpp server. Turn off when using Deepgram only.";
      };
      host = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1";
        description = "Listen address of the whisper server (keep it on loopback).";
      };
      port = lib.mkOption {
        type = lib.types.port;
        default = 8178;
        description = "Port of the whisper server; the daemon is told the same address.";
      };
      threads = lib.mkOption {
        type = lib.types.nullOr lib.types.ints.positive;
        default = null;
        description = "Threads for the whisper server (`-t`); null leaves the default.";
      };
      extraArgs = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [ ];
        description = "Extra `whisper-server` flags.";
      };
      model = lib.mkOption {
        type = lib.types.package;
        # Default per docs/whisper-bench.md; built here, not via flake packages.
        default = (pkgs.callPackage ./whisper-model.nix { })."large-v3-turbo";
        defaultText = lib.literalExpression ''(pkgs.callPackage ./whisper-model.nix { })."large-v3-turbo"'';
        description = "A whisper.cpp model package (one with `passthru.modelName`).";
      };
      package = lib.mkOption {
        type = lib.types.package;
        default = pkgs.whisper-cpp-vulkan;
        defaultText = lib.literalExpression "pkgs.whisper-cpp-vulkan";
        description = "whisper.cpp build providing `whisper-server`.";
      };
    };

    ollamaCloud.environmentFile = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "/run/secrets/ollama-cloud.env";
      description = ''
        File with `OLLAMA_API_KEY=...`, read by the daemon service (systemd `EnvironmentFile`).
        Give a path outside the Nix store; the key is never copied into the store.
        Without it, add the key in the Library, Providers tab.
      '';
    };

    ollamaCloud.keyFile = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "/run/agenix/ollama-cloud";
      description = ''
        File holding only the Ollama Cloud key (for example an agenix secret). It is passed with
        systemd `LoadCredential`, so the daemon reads it without the key entering the Nix store.
        It wins over `environmentFile` and the key added in the app; do not set both options.
      '';
    };

    ollama.url = lib.mkOption {
      type = lib.types.str;
      default = "http://127.0.0.1:11434";
      description = "Address of the local Ollama. Only a loopback address counts as local for local-only interviews.";
    };

    settings = {
      meetingMode = nullable (lib.types.enum enums.meetingMode) "Meeting mode.";
      chatProvider = nullable str "Provider for chat answers.";
      chatModel = nullable str "Model for chat answers.";
      liveProvider = nullable str "Provider for automatic live answers.";
      liveModel = nullable str "Model for automatic live answers.";
      responsePace = nullable (lib.types.enum enums.responsePace) "How soon automatic answers start.";
      responseLength = nullable (lib.types.enum enums.responseLength) "Answer length.";
      responseLanguage = nullable str "Answer language (`Auto` follows the question).";
      autoResponse = nullable (lib.types.enum enums.autoResponse) "Which utterances get an automatic answer.";
      smartFollowUps = nullable bool "Suggest follow-up questions.";
      autoRecordOnCall = nullable bool "Start recording when a call begins.";
      speechProvider = nullable str "Speech-to-text provider (`whisper-local` or `deepgram-stt`).";
      speechModel = nullable str "Speech-to-text model.";
      speechLanguage = nullable str "Speech language code; empty detects it.";
      textScale = nullable (lib.types.ints.between 80 160) "Overlay text size in percent.";
      reduceMotion = nullable bool "Reduce motion.";
      mcpAllowRecord = nullable bool "Let MCP clients start recording.";
      extra = lib.mkOption {
        type = lib.types.attrsOf str;
        default = { };
        example = {
          capture_delay_seconds = "3";
        };
        description = "Settings without a typed option, by key, as strings. Never a secret (`api_key` is refused).";
      };
      enforce = lib.mkOption {
        type = bool;
        default = false;
        description = ''
          Without this the values above are defaults: applied once, and the app wins afterwards
          (a changed Nix value is applied once more). With it they are written at every start and
          the app shows them locked with "Set in Nix".
        '';
      };
    };

    plugin.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Install the Quickshell plugin: through `programs.nixarchy.plugins` when nixarchy is imported,
        else into {file}`~/.config/omarchy/plugins`. Enabling it in the shell and placing its bar
        widget stay a one-time step (`omarchy plugin enable nixarchy.meetingai`).
      '';
    };

    callDetection.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Also install the nixarchy-meeting-mode plugin (call detection) through `programs.nixarchy.plugins`,
        unless you already declare a `meeting-mode` entry there. Needs nixarchy; ignored without it.
      '';
    };

    keybindings = lib.mkOption {
      type = lib.types.attrsOf (lib.types.nullOr lib.types.str);
      default = {
        overlay = "SUPER + ALT + M";
        record = "SUPER + ALT + R";
        stop = "SUPER + ALT + SHIFT + R";
        mic = "SUPER + ALT + SHIFT + M";
        system = "SUPER + ALT + S";
        screenshot = "SUPER + ALT + SHIFT + S";
      };
      description = ''
        Hyprland keys, written to {file}`~/.config/hypr/meetingai-binds.lua`.
        Load it from bindings.lua with `pcall(require, "hypr.meetingai-binds")`.
        Set an entry to null to drop it.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.whisper.enable || cfg.settings.speechProvider != "whisper-local";
        message = "programs.nixarchy-meetingai: settings.speechProvider = \"whisper-local\" needs whisper.enable = true.";
      }
      {
        assertion = !(cfg.settings.extra ? api_key);
        message = "programs.nixarchy-meetingai: settings.extra.api_key would put a secret in the Nix store; use ollamaCloud.keyFile or the app.";
      }
      {
        assertion = cfg.ollamaCloud.keyFile == null || cfg.ollamaCloud.environmentFile == null;
        message = "programs.nixarchy-meetingai: set only one of ollamaCloud.keyFile and ollamaCloud.environmentFile.";
      }
    ];

    home.packages = [ cfg.package ];

    xdg.configFile = lib.mkMerge [
      (lib.mkIf (cfg.plugin.enable && !hasPlugins) {
        "omarchy/plugins/nixarchy.meetingai".source = plugin;
      })
      (lib.mkIf (settingValues != { }) {
        "nixarchy-meetingai/settings.json".text = settingsFile;
      })
      {
        "hypr/meetingai-binds.lua".text = ''
          -- Generated by programs.nixarchy-meetingai. Edits are overwritten.
          if o.cmd_present("nixarchy-meetingai") then
          ${lib.concatStringsSep "\n" (
            lib.mapAttrsToList bindLine (lib.filterAttrs (n: k: k != null && keys ? ${n}) cfg.keybindings)
          )}
          end
        '';
      }
    ];

    programs = lib.optionalAttrs hasPlugins {
      nixarchy.plugins = lib.mkMerge [
        (lib.mkIf cfg.plugin.enable { "nixarchy.meetingai".src = plugin; })
        # Keyed by the id in its manifest, as owners declare it; mkDefault lets their entry win, so it is never added twice.
        (lib.mkIf cfg.callDetection.enable { "nixarchy.meeting-mode".src = lib.mkDefault callDetectionPlugin; })
      ];
    };

    systemd.user.services = {
      nixarchy-meetingai = {
        Unit = {
          Description = "nixarchy-meetingai daemon";
          After = [
            "graphical-session.target"
            "nixarchy-meetingai-whisper.service"
          ];
          PartOf = [ "graphical-session.target" ];
        };
        Service = {
          ExecStart = bin;
          # The user service gets a bare PATH; the subscription CLIs (claude, codex, agy,
          # opencode) live in the user's Nix profile.
          Environment = [
            "PATH=${config.home.profileDirectory}/bin:/etc/profiles/per-user/${config.home.username}/bin:/run/current-system/sw/bin:/run/wrappers/bin"
            "RUST_LOG=nixarchy_meetingai=${cfg.logLevel}"
          ]
          ++ [ "NIXARCHY_MEETINGAI_OLLAMA_URL=${cfg.ollama.url}" ]
          ++ lib.optional cfg.whisper.enable "NIXARCHY_MEETINGAI_WHISPER_URL=http://${whisperAddr}"
          ++ lib.mapAttrsToList (n: v: "${n}=${v}") cfg.extraEnvironment;
          Restart = "on-failure";
          RestartSec = 2;
        }
        // lib.optionalAttrs (cfg.ollamaCloud.environmentFile != null) {
          EnvironmentFile = [ "-${cfg.ollamaCloud.environmentFile}" ];
        }
        // lib.optionalAttrs (cfg.ollamaCloud.keyFile != null) {
          LoadCredential = [ "ollama:${cfg.ollamaCloud.keyFile}" ];
        };
        Install.WantedBy = lib.optional cfg.autostart "graphical-session.target";
      };
    }
    // lib.optionalAttrs cfg.whisper.enable {
      nixarchy-meetingai-whisper = {
        Unit.Description = "whisper.cpp server for nixarchy-meetingai";
        Service = {
          ExecStart = lib.escapeShellArgs (
            [
              "${cfg.whisper.package}/bin/whisper-server"
              "-m"
              modelFile
              "--host"
              cfg.whisper.host
              "--port"
              (toString cfg.whisper.port)
            ]
            ++ lib.optionals (cfg.whisper.threads != null) [
              "-t"
              (toString cfg.whisper.threads)
            ]
            ++ cfg.whisper.extraArgs
          );
          Restart = "on-failure";
          RestartSec = 2;
        };
        Install.WantedBy = lib.optional cfg.autostart "default.target";
      };
    };
  };
}
