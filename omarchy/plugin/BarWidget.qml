import QtQuick
import Quickshell
import qs.Commons
import qs.Ui

BarWidget {
    id: root
    moduleName: "user.osk"

    implicitWidth: button.implicitWidth
    implicitHeight: button.implicitHeight

    WidgetButton {
        id: button
        anchors.fill: parent
        bar: root.bar
        text: "󰌌"
        tooltipText: "On-screen keyboard"
        onPressed: function(buttonCode) {
            Quickshell.execDetached(["omarchy-osk-toggle"])
        }
    }
}
