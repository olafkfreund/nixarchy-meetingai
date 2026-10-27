pragma Singleton
import QtQuick

// Shared by the service and every bar widget: the call (its start) whose chip was dismissed.
QtObject {
    property double dismissed: -1
}
