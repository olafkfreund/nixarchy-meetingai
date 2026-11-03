import QtQuick
import Quickshell

// Only the service, for tests/call-state.sh.
ShellRoot {
    Service { stopGraceMs: 2000 }
}
