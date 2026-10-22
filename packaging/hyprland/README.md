# Lennette private overlay for Hyprland

This is a compositor **patch**, pinned to Hyprland commit
`c7216a9807a2683ce243f6ed14ae01e7e884c426`. It adds the `private_overlay`
window rule. This is built into its own compositor package; it cannot be loaded
as a binary plugin into an existing compositor.

Protected floating, pinned windows render above other application windows,
including fullscreen and other pinned windows. The app starts as a compact dock at the top right; hovering expands the
720x620 conversation panel near the top center. Dashboards start at 1100x760. Monitor captures recompose the public
scene from its surfaces, omitting protected windows, their popups, and
their closing snapshots. The underlying applications remain visible in the
capture, without a replacement rectangle. Direct capture of a protected window
returns Hyprland's denial image. The existing `no_screen_share` rule keeps its
original behavior.

## NixOS and Home Manager setup

Supported system: `x86_64-linux`. No local Lennette or Hyprland source checkout is
needed to build the compositor. Add this input to your own system flake:

```nix
inputs.lennette-private.url = "github:parazeeknova/lennette?dir=packaging/hyprland";
```

Import the modules in your existing NixOS configuration, replacing `alice` with
your Home Manager user. Home Manager must already be integrated with NixOS:

```nix
{ inputs, ... }:
{
  imports = [ inputs.lennette-private.nixosModules.default ];
  programs.lennettePrivateOverlay.enable = true;

  home-manager.users.alice = {
    imports = [ inputs.lennette-private.homeManagerModules.default ];
    programs.lennettePrivateOverlay.enable = true;
    # Optional, if the binary is installed somewhere else:
    # programs.lennettePrivateOverlay.executable = "/absolute/path/to/lennette";

    # Use the NixOS compositor package for the login session.
    wayland.windowManager.hyprland.package = null;
  };
}
```

The modules select the patched compositor and Lua configuration and install a
protected desktop launcher and six compositor bindings. Plugins are supported
when upgraded to this compositor's API, rebuilt against its patched headers,
and listed by exact library path in `programs.lennettePrivateOverlay.allowedPluginPaths`.
The launcher rejects loaded libraries outside that list. Existing Lua configuration can coexist with the
module. Existing Hyprlang configurations must be migrated to Lua first.

Use `inputs.lennette-private.legacyPackages.${system}` to build plugins with the
same Hyprland, compiler, Lua and graphics libraries as the private compositor.
For custom derivations, use that package set's `gcc16Stdenv`, and include its
`hyprland` in build inputs. Updating only the headers is insufficient: plugins
must implement the new render-context, window-presentation and window-state APIs.
Given a list of rebuilt plugin derivations named `plugins`, configure Home Manager:

```nix
wayland.windowManager.hyprland.plugins = plugins;
programs.lennettePrivateOverlay.allowedPluginPaths =
  map (plugin: "${plugin}/lib/lib${plugin.pname}.so") plugins;
```

Capture recomposition bypasses overview workspace hooks, plugin render-stage
callbacks and cached window decorations. Your local desktop retains its plugin
effects; monitor shares show the underlying public scene. Closing snapshots are
omitted from shares because even a public window's glass snapshot can contain
pixels sampled from a protected overlay. The compositor reports each loaded
plugin's library path and capture-compatibility capability to the launcher.

The application is installed separately. From a Lennette checkout, run
`devenv shell -- just install`, and ensure `~/.local/bin` is on your `PATH`.
The configured binary defaults to `${config.home.homeDirectory}/.local/bin/lennette`,
so it follows each Home Manager user's home directory. The launcher requires
the app to advertise `LENNETTE_PRIVATE_OVERLAY=1` in its help output; this
suppresses the desktop tray icon. Quit an existing Lennette instance before
using private mode. `just install` preserves a Nix-managed desktop launcher.
On NixOS it also embeds the application's runtime library paths, so launching
from an app menu does not depend on an active development shell.

Build your system before activation. Replace `my-host` with the host name from
your own flake, and run these commands from that flake's directory:

```sh
nix flake update lennette-private
nixos-rebuild build --flake .#my-host
sudo nixos-rebuild switch --flake .#my-host
# Save your work, log out of Hyprland, and log in again.
lennette-private-user --check
lennette-private-user
```

Commit your `flake.lock` to pin the published integration and all compositor
inputs. To deliberately pin the input URL itself, use
`github:parazeeknova/lennette/<commit>?dir=packaging/hyprland`.
Do not make this flake's `nixpkgs` follow your system input: the compositor's
libraries must match its pinned upstream revision.

A configuration reload or NixOS switch alone does not replace the already
running compositor. Until you log out and back in, the old compositor may show
`unknown field 'private_overlay'` after loading the new configuration. The
launcher refuses to start the app in that session. No switch or logout is
performed by the build or QA scripts. The additional desktop entry is named
**Lennette (private overlay)**. Home Manager also routes the ordinary
**Lennette** app-menu entry through the protected launcher.

The launcher verifies that its Wayland display and Hyprland instance match,
requires the compositor to accept the protection rule, checks loaded plugin paths,
and removes X11/headless overrides before starting Lennette. Use this launcher
for launches and shortcut commands. The Home Manager module owns `lennette.lua`
so Lennette's automatic shortcut installer cannot replace it. Configure these
bindings in Nix; application-side remapping cannot overwrite the managed file.

| Keys | Action |
| --- | --- |
| Ctrl+Shift+D | Dashboard command |
| Ctrl+Shift+Backslash | Window command |
| Ctrl+Shift+I | Focus input |
| Ctrl+Shift+M | System audio command |
| Ctrl+Shift+A | Voice input command |
| Ctrl+Shift+S | Screenshot command |

These forward Lennette's existing command-line actions to its running instance.
The window command hides the overlay on a private special workspace and restores
it to the active workspace, floating and pinned. Ctrl+Shift+I also restores a
hidden overlay and focuses its input. The Hyprland binding uses physical keycode
51 for the US backslash key so Shift does not change its match into `bar`.
The dashboard command opens the dashboard. The compositor does not implement the
application's audio, transcription, or meeting-response features.

## Test and inspect

```sh
cd lennette
nix flake check path:./packaging/hyprland --no-build
nix build --no-link path:./packaging/hyprland#checks.x86_64-linux.patch \
  path:./packaging/hyprland#checks.x86_64-linux.launcher
nix develop path:./packaging/hyprland
packaging/hyprland/tests/build-fixture.sh
# The dev shell supplies protocol sources from the pinned upstream input.
# Optional: clone an editable compositor checkout (applies the patch in place):
git clone --recursive https://github.com/hyprwm/Hyprland ref/Hyprland
git -C ref/Hyprland checkout c7216a9807a2683ce243f6ed14ae01e7e884c426
git -C ref/Hyprland submodule update --init --recursive
BUILD_TYPE=Debug BUILD_JOBS=2 packaging/hyprland/build.sh
packaging/hyprland/tests/smoke.py --lennette-binary target/release/lennette
```

`smoke.py` launches an isolated, nested compositor with two headless outputs.
It requires a parent Wayland compositor (`--parent-socket`, default
`$WAYLAND_DISPLAY`) and an accessible DRM render node (`--drm-node`, default
`$TEST_DRM_NODE` or the first accessible `/dev/dri/renderD*`). Set `--compositor`
to the Nix-built `bin/Hyprland` to test the packaged executable. It briefly
focuses its own nested window to photograph
local visibility. Its outer window is explicitly floated and bounded to
960x640. Virtual input is sent only to the isolated compositor.

The tests exercise wlr screencopy and ext image copy capture with both SHM and
DMA-BUF, direct window denial, popups, overlap with a public pinned window,
pointer clicks, cross-output movement, fractional scaling and rotation,
workspace switching, and closing snapshots. The real application test disables
protection temporarily as a positive control, proving that the test would
otherwise see its pixels. This happens only inside the isolated compositor.

`--shortcut-module FILE` additionally loads the generated Home Manager Lua module
and exercises real virtual-keyboard window/input/dashboard bindings.
`--plugins FILE` loads a JSON array of absolute plugin library paths into the
isolated compositor and verifies they all load together before capture tests.
`--doty-config-dir /path/to/doty/modules/features/wm/hyprland/hypr`
checks the input, binding, decoration, layout, miscellaneous, window, and
workspace modules with the selected plugins. This does not run desktop autostart
commands or validate physical monitor configuration.

Artifacts are written below `target/hyprland-private-qa` (or `--artifacts`):
`results.json`, protocol-exported PPM frames, local-display photographs, and
compositor/application logs. Failed runs never leave `passed: true` from an
older run. Logs and images are local test artifacts and are not committed.

## Protection boundary

This cannot guarantee “undetectable under all circumstances.” It protects the
patched compositor's supported window/monitor capture paths. The compositor
must be running, its rule must remain enabled, and launches must use the
protected configuration. Ordinary launches on an unpatched desktop are unsafe
for this purpose. Changing plugin binaries or protection rules after launch
invalidates the tested configuration; the launcher is a launch-time check.

Window metadata, notifications, input-method windows, clipboard/drag
surfaces, and copies of Lennette's content displayed by another application are
outside this window rule. Capture of a parent/nested compositor, raw GPU/KMS
capture, external cameras/capture cards, other compositors, and privileged local
software are outside its boundary. Do not infer full-session protection from
the nested test: the parent compositor deliberately sees the local overlay.

No live receiver-side Google Meet or Zoom call is covered by the protocol tests.
After activating the patched desktop, verify a private test share from a second
device for each meeting application and share mode you intend to use. Audio,
transcription, and AI response accuracy require separate validation.

## Overlay controls

The app opens as a compact pill at the top right of the active monitor. Hover
to reveal the centered panel; it retracts after leaving, while an active input
or open dropdown keeps it expanded. Starting a recording returns it to the
corner. Hyprland animates the window movement and the panel fades in.

The footer offers **Full**, **Follow-ups**, and **Hide** modes. The dashboard
and screenshot controls are icons at the left and right of the input. Dark
mode uses neutral charcoal; glass elements preserve their alpha and the app's
window rules disable compositor blur. Text uses the system UI font.

Recording is disabled until a configured speech provider has a key. All start
paths share that check; recording shows a red stop square, elapsed time, and
an audio-level waveform. Removing a key never prevents stopping an active
recording. The provider/model picker selects the provider used by chat requests.

## Overview compatibility regression

The `hyprbars` and `scrolloverview` compatibility patches in `~/doty` exchange
button sizes through `hyprbarsOverviewButtonSizesV1`. Overview must never cast
hyprbars' `g_pGlobalState` to a copied C++ structure: changes to `SHyprButton`
change the vector stride and can corrupt memory while drawing floating windows.
Both plugins must be rebuilt with their patches.

Run `tests/overview-compat.py` from a Wayland session, supplying `--compositor`,
`--plugins` (a JSON array of absolute plugin library paths), `--lennette`,
`--client-dir` (from `tests/build-fixture.sh`), and `--artifacts`. It starts an
isolated compositor and exercises both overviews with three decorated floating
windows and multiple titlebar buttons, first without Lennette, then in normal
and private modes. It also checks normal close-to-tray, menu/shortcut restore,
and private-mode exit without a tray. It never reloads the desktop's plugins.

Normal mode keeps its tray and control socket alive when the overlay is closed;
use **Quit** in the tray menu to exit. Private mode has no tray and closing the
last window exits. Syntax grammars warm in the background so decoding them does
not delay the initial window.

On Intel-boot/NVIDIA hybrid laptops, Lennette uses the installed Intel Vulkan
ICD by default. This avoids waking a suspended discrete GPU during startup.
Explicit Vulkan driver settings, `ZED_DEVICE_ID`, `DRI_PRIME`, and NVIDIA offload
settings take precedence; other hardware keeps the Vulkan loader's default.
