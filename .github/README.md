<h1 align="center"><img src="https://swr2.itssingularity.com/lennette/lennette_app_icon.png" width="36" height="36" alt="" /> Lennette</h1>

<p align="center">A native desktop copilot for meetings, interviews, and live discussions.<br />Live transcripts, useful context, and answers when you need them. Built with Rust + GPUI.</p>


## What it does

- **Listen to both sides.** Microphone and system-audio transcription with separate speaker context.
- **Answer with context.** Streamed suggestions using your prompt, reusable profiles, previous conversations, screenshots, and PDF/text/image attachments.
- **Make it yours.** General and Interview modes, editable answer/follow-up prompts, a cached live model selector, and **No context** when you want a clean start.
- **Choose when it responds.** On questions or after substantive pauses, with Fast / Balanced / Relaxed pacing.
- **Hold to dictate.** Use the input microphone or **Ctrl+Shift+Space**; release inserts a draft for review.
- **Stay out of the way.** Compact overlay, transparent glass theme, global shortcuts, local history, and exports.

Bring your own keys: **Google Gemini** or **Merge Gateway** for chat; **Deepgram** for speech.

<p align="center">
  <img src="https://swr2.itssingularity.com/lennette/lennette_header.png" alt="Lennette — know exactly what to say, live" width="100%" />
</p>

## Getting started

Download a build from [Releases](https://github.com/parazeeknova/lennette/releases). SHA-256 checksums are included.

| Platform | Run it | Current support |
| --- | --- | --- |
| Linux x86_64 | Extract, then `./lennette` | X11 / Wayland; PulseAudio or PipeWire-Pulse for system audio. |
| Windows x86_64 | Run the portable `.exe`, or extract the ZIP | Windows 10 **2004+** / Windows 11; native hotkeys and WASAPI audio. |
| NixOS / Hyprland | Build below; optionally install the compositor patch | Protected launcher and capture exclusion with patched Hyprland. |

Open **Settings → AI & Keys**, add your keys, and select a model. Create background profiles in **Settings → Profiles**, then select one beside **General / Interview** in the overlay.

Linux release builds need a recent glibc desktop with Vulkan, fontconfig, XKB, ALSA, and X11/Wayland libraries. Install `pulseaudio-utils` for system audio and `grim` for Hyprland screenshots. The archive includes its build/glibc versions. Windows builds need no installer, WebView, Node.js, or separate Visual C++ runtime. macOS and other architectures have no supported release yet.

### Screen sharing

**Windows:** the overlay and dashboard use OS capture exclusion, including for full-monitor sharing on capture paths that honor it. They stay visible locally. Private launch is the default, with no tray icon, taskbar entry, or Alt+Tab entry. The new Windows backend still needs real meeting/audio validation; check the receiver's view in your meeting software. [Windows support and limits](https://github.com/parazeeknova/lennette/blob/main/docs/windows.md).

**Linux:** capture exclusion requires the **patched Hyprland compositor** below. Ordinary X11/Wayland launches do not provide it. [Protection boundary](https://github.com/parazeeknova/lennette/blob/main/packaging/hyprland/README.md#protection-boundary).

## Build from source

Rust **1.99.1** is pinned in `rust-toolchain.toml`. A native desktop/GPU is required.

### Linux / NixOS

With [Nix](https://nixos.org/download/) and [devenv](https://devenv.sh/getting-started/) installed:

```sh
git clone https://github.com/parazeeknova/lennette.git
cd lennette
devenv shell -- cargo run --locked
# Install the optimized binary, icon, and desktop entry locally:
devenv shell -- just install
```

The shell supplies native libraries; installation preserves their Nix runtime paths. Keep `~/.local/bin` on your `PATH`. Without Nix, install the packages in [release.yml](https://github.com/parazeeknova/lennette/blob/main/.github/workflows/release.yml) and use `cargo run --locked`.

### Windows

Install [Rust](https://rustup.rs/), Visual Studio Build Tools' **Desktop development with C++** workload, and a Windows SDK. In Developer PowerShell:

```powershell
git clone https://github.com/parazeeknova/lennette.git
cd lennette
cargo run --locked
# Portable release build:
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --locked --release --target x86_64-pc-windows-msvc
.\target\x86_64-pc-windows-msvc\release\lennette.exe
```

## Hyprland / your own config

The optional `private_overlay` patch excludes protected windows and their popups from supported capture paths. It is a pinned compositor build, not a loadable plugin.

Add this input to your system flake:

```nix
inputs.lennette-private.url = "github:parazeeknova/lennette?dir=packaging/hyprland";
```

With integrated Home Manager, add to your NixOS configuration:

```nix
{ inputs, ... }: {
  imports = [ inputs.lennette-private.nixosModules.default ];
  programs.lennettePrivateOverlay.enable = true;
  home-manager.users.YOUR_USER = {
    imports = [ inputs.lennette-private.homeManagerModules.default ];
    programs.lennettePrivateOverlay.enable = true;
    # Optional: programs.lennettePrivateOverlay.executable = "/absolute/path/to/lennette";
    wayland.windowManager.hyprland.package = null;
  };
}
```

Install Lennette separately, update your flake lock, rebuild, then **log out and back in**. Run `lennette-private-user --check`, then `lennette-private-user`. The module adds the protected launcher and bindings to your Lua config. Keep this input's pinned `nixpkgs`; plugins must be rebuilt for its compositor and explicitly allowed. [Full integration guide](https://github.com/parazeeknova/lennette/blob/main/packaging/hyprland/README.md).

## Shortcuts

| Keys | Action |
| --- | --- |
| Ctrl+Shift+Backslash | Show / hide |
| Ctrl+Shift+D | Dashboard |
| Ctrl+Shift+I | Focus input |
| Ctrl+Shift+A | Start / stop listening |
| Ctrl+Shift+M | Toggle system audio |
| Ctrl+Shift+S | Attach screenshot |
| Ctrl+Shift+Space | Hold to dictate with input focused |

While hidden, only **Show / hide** works; active meeting capture continues. Wayland hotkeys need a supported desktop portal or compositor bindings. The Hyprland module configures protected bindings using the physical backslash keycode.

## Your data

History, settings, profiles, and the AES-256-GCM credential vault live locally in your user data directory. The vault key is a local file. Audio goes to your speech provider; generation sends your prompt, selected context/attachments, bounded chat history, and both sides of the live transcript to your chat provider. Suggestions stay in the app; Lennette does not speak or send them into your call.

## Development

```sh
devenv shell -- cargo test --locked --all-targets
devenv shell -- cargo clippy --locked --all-targets -- -D warnings
devenv shell -- cargo fmt --all -- --check
```

Pushes and merges to `main` build/test Linux and Windows on `-latest` GitHub runners. Both builds must pass before publishing a release with the banner, Linux archive, portable EXE/ZIP, and checksums. Manual runs on other branches build without publishing.

<p align="center">
  <img src="https://swr2.itssingularity.com/lennette/lennette.png" alt="Lennette himself" width="100" />
  <br />
  <sub>Know exactly what to say, live.</sub>
</p>

<p align="center">
  <img src="https://swr2.itssingularity.com/lennette/lennette_footer.png" alt="Lennette" width="100%" />
</p>
