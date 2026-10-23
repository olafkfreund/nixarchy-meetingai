# Windows support

The native Windows backend implements microphone input, WASAPI output loopback,
provider streaming/transcription, screenshots, context profiles, attachments,
history, prompts, hotkeys, tray controls, and hide/show. Shared app features use
the same Rust implementation as Linux.

## Portable build

The release includes a setup wizard (`lennette-windows-x86_64-setup.exe`), a
standalone x86_64 executable and a ZIP. The wizard installs for the current user
under `%LOCALAPPDATA%\Programs\Lennette`, adds a Start menu shortcut, and offers
an optional desktop shortcut. It needs no administrator rights. Run it again to
upgrade; Windows Settings can uninstall it. Settings, history and credentials
are preserved on upgrade and uninstall. The existing `lenny` PowerShell setup
remains a separate installation method.

The executable statically links the Microsoft C runtime and uses Windows system
DLLs and Direct3D, with no WebView dependency. Windows 10 version 2004 or newer,
or Windows 11, is required.
Settings and the encrypted vault use the current user's application-data folder;
"portable" describes installation, not storing credentials beside the EXE.

Build with the MSVC toolchain, Visual Studio's C++ workload and a Windows SDK:

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --locked --release --target x86_64-pc-windows-msvc
```

GPUI looks for the SDK's `fxc.exe` when compiling release shaders. If your SDK is
installed in a different location, set `GPUI_FXC_PATH` to its full path. CI
finds it automatically.

To build the setup wizard, install [Inno Setup 6.3+](https://jrsoftware.org/isinfo.php)
and run:

```powershell
./scripts/build-windows-installer.ps1 -Binary target/x86_64-pc-windows-msvc/release/lennette.exe
```

Set `ISCC_PATH` to `ISCC.exe` if it is not in the default Inno Setup 6 folder.
The output is `dist/lennette-windows-x86_64-setup.exe`.

## Window frame and dragging

GPUI draws the same 22px window curve on Windows and Linux, with a dark outer
hairline and light inner hairline. Native Windows corner clipping, frame material
and accent backdrops are disabled so they cannot compete with that curve or tint
fully transparent pixels. The transparency slider changes surface alpha while
text stays readable. Native resize handles remain available. The top pill's grip and empty space move the window using
Windows caption dragging. Its new-chat, model, prompt, profile and recording
buttons keep their own click targets. The dashboard header also supports dragging.
The pill and composer use paired outer edges; pill options have no individual
outlines and inner composer controls keep subtle outlines, including on hover.

The Windows EXE embeds the existing app artwork as a multi-size icon resource.
Setup uses the same artwork for its own icon, installed shortcuts and uninstall
entry. Linux setup registers a desktop entry and installs the shipped PNG into
the user's hicolor icon directory; the Home Manager integration supplies it too.

## Capture exclusion

Every overlay/dashboard HWND receives `SetWindowDisplayAffinity` with
`WDA_EXCLUDEFROMCAPTURE` before being shown. The policy is read back with
`GetWindowDisplayAffinity`. The actual OS build is checked to reject the older
black-rectangle fallback. If Windows rejects the policy, the app refuses to show the
unprotected window. The policy is reapplied and checked before each hide/show
restore. Privacy windows use tool-window styling to omit taskbar and
Alt+Tab entries. Hide/show uses `ShowWindow`, and hidden-state shortcut filtering
is shared with Linux.

### Sharing your complete screen

On Windows capture paths that honor display affinity, sharing a **whole monitor**
should omit the overlay and dashboard while you continue to see and use them on
that monitor. Exclusion applies to the window pixels in desktop capture, not only
to sharing a selected application. It is the Windows equivalent of the supported
capture exclusion supplied by Lennette's patched Hyprland integration.

Setup installs two Start-menu entries sharing one executable and data:

- **Lennette** (`--normal`): branded tray; uses your Hide app icon preference.
- **Lennette (Privacy)** (`--privacy`): no tray, taskbar or Alt+Tab entry.

Launching the other entry switches the running instance's mode. Window capture
exclusion remains active in both. The tray belongs to Explorer and can appear in
a desktop share. Closing Normal hides to the tray; closing Privacy exits.
Ctrl+Shift+Backslash still restores an explicitly hidden overlay. Native file
pickers, system notifications and other applications are not covered by the app's
display-affinity policy.

Microsoft documents this API as capture protection for supported public OS capture
paths, not a security/DRM guarantee. Privileged software, external cameras/capture
cards, process/window enumeration, and capture paths that ignore this policy remain
outside its boundary. [Microsoft API documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity).

## Settings, capture and providers

General settings register per-user startup on Windows or Linux and select its
Normal/Privacy mode. Hide app icon controls the taskbar or X11 dock; Wayland
compositor policy may require window rules.

Audio has background device discovery, selectors for microphone and system
output, System default and Refresh. Selected devices are used on the next
recording. Windows uses WASAPI loopback; Linux system audio needs PulseAudio or
PipeWire-Pulse with `pactl` and `parec`. Microphone device IDs use the same CPAL
backend as capture. Legacy saved names still work. Local microphone, loopback and
screen tests discard samples/frames without uploading them. Windows microphone
privacy settings open directly from Audio; detection alone is not a permission grant.

Custom AI providers use OpenAI message requests, POST/PUT/PATCH, configurable
authentication, OpenAI SSE or JSON-pointer/plain-text batch responses. Custom STT
supports raw WAV, multipart `file`/`model`, or JSON base64 `audio`/`model`/`language`.
Live speech requires the Deepgram-compatible stereo PCM WebSocket protocol;
disable streaming for utterance-based batch transcription. Push to talk always
uses the HTTP endpoint. Custom keys and configuration are encrypted in the vault.

Response settings include Short/Medium/Auto detail, language, scrolling, smart
follow-ups, and opt-in automatic meeting recaps. Answer preferences are captured
for each new conversation. Recaps wait for final transcript words and save beside
the meeting with a retry action on failure. Recaps and follow-ups use extra AI
requests. Recap context is limited to 120,000 characters for very long transcripts;
the omission is stated in the request.

Prompts use searchable square cards with create/edit/delete controls. Deleted
built-in prompts stay deleted after restart and custom prompts appear in the
overlay picker. About links to the landing page and checks the latest stable
GitHub release on first visit or on request, without downloading an update.

## Verification

Windows CI compiles, lints, tests and builds the release using `windows-latest`.
Windows-specific tests cover virtual-key mapping, named-pipe command delivery,
single-instance locking and applying/reading back window capture exclusion. The
release smoke check captures full-desktop pixels through GDI with a visible,
unprotected positive control. It checks that the locally visible overlay/dashboard
are omitted and that a black-rectangle fallback is not used. This exercises one
Windows capture path; it does not replace a receiver-side check in your meeting app.
It also checks Windows frame attributes and the pill's native caption hit-tests,
including that buttons and chat content remain independent of the drag region.
Packaging checks that the EXE starts and imports no separately installed Visual
C++ runtime DLL. The setup check installs the exact release binary in an isolated
directory, checks its Start menu shortcut, repeats setup as an upgrade, and
uninstalls while preserving unrelated user files. It refuses to overwrite an
existing setup installation. A Linux cross-check cannot prove Windows runtime behavior.

Before relying on a Windows build during a meeting, test on the actual desktop:

1. Confirm microphone and system playback produce separate transcript entries.
2. Dictate with the input button and keyboard, including release, Escape, focus
   loss, and dictation during an active meeting.
3. Hide the overlay, try all other global shortcuts, then restore it with the
   visibility shortcut. Only the latter should act while hidden.
4. Share the full monitor and a selected app in your meeting software; verify the
   receiver on a second device sees the underlying app without Lennette.
5. Open the dashboard, menus and screenshots while sharing; repeat after changing
   monitors, DPI, graphics drivers or capture software.

Local validation on Windows 11 Pro build 26300 covers startup, automated Rust
tests, native frame/caption checks, capture exclusion and global shortcuts.
Receiver-side Meet/Zoom calls and physical microphone/loopback transcription
still require the manual checks above. CI failures block publication.
