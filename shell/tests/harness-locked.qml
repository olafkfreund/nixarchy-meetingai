import QtQuick
import Quickshell

// For tests/smoke.sh --locked: the daemon says chat_provider, chat_model and text_scale are set in Nix.
// Pick a provider as the Settings dropdown does and open Settings; smoke.sh checks no settings.set for a locked key left.
ShellRoot {
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 2000; running: true; onTriggered: { overlay.setProvider(false, "ollama"); overlay.setProvider(true, "ollama"); overlay.showLibrary("settings"); } }
}
