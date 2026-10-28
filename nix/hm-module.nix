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
  # The daemon dials 127.0.0.1 (or ::1) when the server listens on every address; IPv6 gets brackets.
  daemonHost =
    {
      "0.0.0.0" = "127.0.0.1";
      "::" = "::1";
    }
    .${cfg.whisper.host} or cfg.whisper.host;
  whisperAddr =
    (
      if lib.hasInfix ":" daemonHost && !lib.hasPrefix "[" daemonHost then
        "[${daemonHost}]"
      else
        daemonHost
    )
    + ":${toString cfg.whisper.port}";
  # systemd expands % and $ in ExecStart words.
  unitEscape = lib.replaceStrings [ "%" "$" ] [ "%%" "$$" ];
  # The app only offers an Ollama at https, or http on loopback (see endpoint_allowed in the daemon).
  ollamaUrlOk =
    lib.match "(https://.*|http://(localhost|127\\.0\\.0\\.1|[[]::1[]])(:[0-9]+)?(/.*)?)" (
      lib.toLower cfg.ollama.url
    ) != null;
  speechBaseUrlOk =
    cfg.settings.speechBaseUrl == null
    ||
      lib.match "(https://.*|http://(localhost|127\\.0\\.0\\.1|[[]::1[]])(:[0-9]+)?(/.*)?)" (
        lib.toLower cfg.settings.speechBaseUrl
      ) != null;

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
    calendar_enabled = calendarEnabled;
    speech_provider = speechProvider;
    speech_model = speechModel;
    speech_language = speechLanguage;
    speech_base_url = speechBaseUrl;
    deepgram_region = cfg.deepgram.region;
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
  daemonEnv = [
    "PATH=${config.home.profileDirectory}/bin:/etc/profiles/per-user/${config.home.username}/bin:/run/current-system/sw/bin:/run/wrappers/bin"
    "RUST_LOG=nixarchy_meetingai=${cfg.logLevel}"
  ]
  ++ [ "NIXARCHY_MEETINGAI_OLLAMA_URL=${cfg.ollama.url}" ]
  ++ [ "NIXARCHY_MEETINGAI_PARAKEET_DIR=${cfg.parakeet.model}" ]
  ++ lib.optional cfg.whisper.enable "NIXARCHY_MEETINGAI_WHISPER_URL=http://${whisperAddr}"
  ++ lib.optional (
    cfg.ollamaCloud.keyFile != null
  ) "NIXARCHY_MEETINGAI_OLLAMA_KEY_FILE=${cfg.ollamaCloud.keyFile}"
  ++ lib.optional (
    cfg.deepgram.keyFile != null
  ) "NIXARCHY_MEETINGAI_DEEPGRAM_KEY_FILE=${cfg.deepgram.keyFile}"
  ++ lib.mapAttrsToList (n: v: "${n}=${v}") cfg.extraEnvironment;

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

    parakeet.model = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./parakeet-model.nix { };
      defaultText = lib.literalExpression "pkgs.callPackage ./nix/parakeet-model.nix { }";
      description = "The Parakeet speech model for `speechProvider = \"parakeet-local\"`: a directory with `encoder.int8.onnx`, `decoder.int8.onnx`, `joiner.int8.onnx` and `tokens.txt` (NVIDIA Parakeet TDT 0.6B v3, CC-BY-4.0). The daemon gets it as `NIXARCHY_MEETINGAI_PARAKEET_DIR`. Once Parakeet has been used the model stays loaded (about 1.1 GB RAM) while the daemon runs.";
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
        description = ''
          Listen address of the whisper server (keep it on loopback). The daemon is told 127.0.0.1
          when this is `0.0.0.0` and `::1` when it is `::`; an IPv6 address is bracketed in its URL.
        '';
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
        File holding only the Ollama Cloud key (for example an agenix secret). The daemon reads
        this path itself (`NIXARCHY_MEETINGAI_OLLAMA_KEY_FILE`), so the key never enters the Nix
        store; a missing file just means no key (logged once at warn). It wins over `environmentFile`
        and the key added in the app; do not set both options.
      '';
    };

    deepgram.keyFile = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "/run/agenix/deepgram";
      description = "Path to a Deepgram key file, read by the daemon at runtime. A vault key takes precedence.";
    };

    deepgram.region = lib.mkOption {
      type = lib.types.nullOr (
        lib.types.enum [
          "us"
          "eu"
        ]
      );
      default = null;
      description = "Deepgram API region; EU requests opt out of model improvement.";
    };

    ollama.url = lib.mkOption {
      type = lib.types.str;
      default = "http://127.0.0.1:11434";
      description = ''
        Address of the Ollama server, `https://...` or `http://` on localhost, 127.0.0.1 or [::1]
        (anything else is refused by an assertion, since the app would hide the provider). It wins
        over a base URL stored in the app. Only a loopback address counts as local for
        local-only interviews.
      '';
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
      calendarEnabled = nullable bool "Name sessions from the calendar (read-only, through `gog`; off by default).";
      speechProvider = nullable str "Speech-to-text provider (`parakeet-local`, `whisper-local`, `deepgram-stt`, or `openai-compatible-stt`).";
      speechModel = nullable str "Speech-to-text model.";
      speechLanguage = nullable str "Speech language code; empty detects it.";
      speechBaseUrl = nullable str "Base URL for OpenAI-compatible speech; must be set for that provider.";
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

          A default that you remove and later add back with the same value is not applied again
          (the app remembers a hash of each value it has applied). To force it, change the value
          or set this option.
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

    echoCancel.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Load PipeWire's WebRTC echo canceller with the default sink's monitor as the reference and
        expose the result as the `meetingai-echo-cancel-source` source, which the daemon records as the
        microphone when present. The system default source is not changed.
        It takes effect after PipeWire restarts or at the next login
        (`systemctl --user restart pipewire pipewire-pulse wireplumber`).
      '';
    };

    callDetection.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Also install the nixarchy-meeting-mode plugin (call detection) through `programs.nixarchy.plugins`,
        unless you already declare a `nixarchy.meeting-mode` entry there. Needs nixarchy; ignored without it.
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
        assertion = ollamaUrlOk;
        message = "programs.nixarchy-meetingai: ollama.url must be https, or http on localhost, 127.0.0.1 or [::1].";
      }
      {
        assertion = speechBaseUrlOk;
        message = "programs.nixarchy-meetingai: settings.speechBaseUrl must be https, or http on localhost, 127.0.0.1 or [::1].";
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
      (lib.mkIf cfg.echoCancel.enable {
        "pipewire/pipewire.conf.d/60-meetingai-echo-cancel.conf".text = ''
          # Generated by programs.nixarchy-meetingai. Edits are overwritten.
          context.modules = [
            {
              name = libpipewire-module-echo-cancel
              # A missing or failing module must never stop PipeWire at login.
              flags = [ ifexists nofail ]
              args = {
                library.name = aec/libspa-aec-webrtc
                # The default sink's monitor is the reference; the capture side follows the default source.
                monitor.mode = true
                capture.props = {
                  node.name = "meetingai-echo-cancel-capture"
                }
                source.props = {
                  node.name = "meetingai-echo-cancel-source"
                  node.description = "Meeting AI echo-cancelled mic"
                }
                # The module always makes a sink too; name it so it is recognisable in the sound menu.
                sink.props = {
                  node.name = "meetingai-echo-cancel-sink"
                  node.description = "Meeting AI echo canceller (not an output)"
                }
              }
            }
          ]
        '';
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
        (lib.mkIf cfg.callDetection.enable {
          "nixarchy.meeting-mode".src = lib.mkDefault callDetectionPlugin;
        })
      ];
    };

    systemd.user.services = {
      nixarchy-meetingai = {
        Unit = {
          Description = "nixarchy-meetingai daemon";
          # autostart = false must not leave speech-to-text off.
          Wants = lib.optional cfg.whisper.enable "nixarchy-meetingai-whisper.service";
          # A rebuild with other settings or environment restarts the daemon.
          X-Restart-Triggers = [
            (pkgs.writeText "nixarchy-meetingai-settings.json" settingsFile)
            (builtins.toJSON daemonEnv)
          ];
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
          Environment = daemonEnv;
          Restart = "on-failure";
          RestartSec = 2;
        }
        // lib.optionalAttrs (cfg.ollamaCloud.environmentFile != null) {
          EnvironmentFile = [ "-${cfg.ollamaCloud.environmentFile}" ];
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
            ++ map unitEscape cfg.whisper.extraArgs
          );
          Restart = "on-failure";
          RestartSec = 2;
        };
        Install.WantedBy = lib.optional cfg.autostart "default.target";
      };
    };
  };
}
