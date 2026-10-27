# nixarchy-meetingai

An ambient meeting copilot for [Omarchy](https://omarchy.org) on NixOS
(Nixarchy). A headless Rust daemon records and transcribes a call, answers
questions about it, and writes rolling notes. The UI is a Quickshell
(omarchy-shell) plugin, `nixarchy.meetingai`.

Derived from [lennette](https://github.com/parazeeknova/lennette) by
parazeeknova. Licensed under AGPL-3.0, see `LICENSE`.

## Install

Add the flake and the Home Manager module:

```nix
{
  inputs.nixarchy-meetingai.url = "github:olafkfreund/mixarchy-meetingai";

  # in your Home Manager configuration
  imports = [ inputs.nixarchy-meetingai.homeManagerModules.default ];
  programs.nixarchy-meetingai.enable = true;
}
```

This installs the `nixarchy-meetingai` binary, two user services
(`nixarchy-meetingai` and `nixarchy-meetingai-whisper`), the Quickshell plugin
and a Hyprland key fragment.

Options:

| Option | Default |
| --- | --- |
| `programs.nixarchy-meetingai.package` | this flake's package |
| `programs.nixarchy-meetingai.whisper.model` | `whisper-model-small-en` |
| `programs.nixarchy-meetingai.whisper.package` | `pkgs.whisper-cpp-vulkan` |
| `programs.nixarchy-meetingai.whisper.port` | `8178` |
| `programs.nixarchy-meetingai.keybindings` | see below |

## Providers

| Provider | Sign in |
| --- | --- |
| Claude (subscription) | run `claude` once and log in; the daemon spawns the CLI |
| ChatGPT / Codex (subscription) | run `codex login` and choose ChatGPT; the daemon spawns the CLI |
| Gemini | API key, set in the Library, Providers tab |
| Ollama | none; uses `http://127.0.0.1:11434` |

API keys live in the encrypted local vault and never cross the daemon socket.

## Offline mode

Speech-to-text is `whisper-server` on `127.0.0.1:8178` with the
`ggml-small.en` model, built into the Nix store, so transcription needs no
network. Pair it with an Ollama model for answers and notes with no network
at all. Deepgram stays available as a cloud alternative.

## Keybindings

The module writes `~/.config/hypr/meetingai-binds.lua`. Load it once from your
`bindings.lua`:

```lua
pcall(require, "hypr.meetingai-binds")
```

| Action | Key |
| --- | --- |
| Overlay | `SUPER + ALT + M` |
| Start recording | `SUPER + ALT + R` |
| Stop recording | `SUPER + ALT + SHIFT + R` |
| Toggle microphone | `SUPER + ALT + SHIFT + M` |
| Toggle system audio | `SUPER + ALT + S` |
| Region screenshot | `SUPER + ALT + SHIFT + S` |

Override with `programs.nixarchy-meetingai.keybindings.<name> = "…"`, or set
one to `null` to drop it. The same actions are available from a shell:
`nixarchy-meetingai ctl state.get`.

## Development

```sh
devenv shell -- cargo test --workspace
nix build .#default
```
