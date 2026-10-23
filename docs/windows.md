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

The transparent surface extends through the native frame; Windows 11's extra
outline is disabled and rounded corners are requested. Native resize handles
remain available. The top pill's grip and empty space move the window using
Windows caption dragging. Its new-chat, model, prompt, profile and recording
buttons keep their own click targets. The dashboard header also supports dragging.

## Capture exclusion

Every overlay/dashboard HWND receives `SetWindowDisplayAffinity` with
`WDA_EXCLUDEFROMCAPTURE` before being shown. The policy is read back with
`GetWindowDisplayAffinity`. The actual OS build is checked to reject the older
black-rectangle fallback. If Windows rejects the policy, the app refuses to show the
unprotected window. The policy is reapplied and checked before each hide/show
restore. App windows also use tool-window styling to omit taskbar and
Alt+Tab entries. Hide/show uses `ShowWindow`, and hidden-state shortcut filtering
is shared with Linux.

### Sharing your complete screen

On Windows capture paths that honor display affinity, sharing a **whole monitor**
should omit the overlay and dashboard while you continue to see and use them on
that monitor. Exclusion applies to the window pixels in desktop capture, not only
to sharing a selected application. It is the Windows equivalent of the supported
capture exclusion supplied by Lennette's patched Hyprland integration.

Windows launches privately by default. The tray is disabled because its pixels
belong to Explorer and would otherwise be visible in a full-screen share. App
windows are also omitted from the taskbar and Alt+Tab. No environment variable or
compositor patch is needed for the default Windows launch:

```powershell
.\lennette.exe
```

To explicitly opt into a visible tray (which can appear in a desktop share):

```powershell
$env:LENNETTE_PRIVATE_OVERLAY = '0'
.\lennette.exe
```

Quit an existing instance before changing launch mode. In default private mode,
close the app window to exit.
Ctrl+Shift+Backslash still restores an explicitly hidden overlay. Native file
pickers, system notifications and other applications are not covered by the app's
display-affinity policy.

Microsoft documents this API as capture protection for supported public OS capture
paths, not a security/DRM guarantee. Privileged software, external cameras/capture
cards, process/window enumeration, and capture paths that ignore this policy remain
outside its boundary. [Microsoft API documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity).

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
