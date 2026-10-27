#!/usr/bin/env bash
# qmllint with the omarchy tree (as qs.Commons / qs.Ui) and quickshell on the
# import path. Expects qmllint, QT_QML dir and the quickshell qml dir; override
# with OMARCHY_SHELL_TREE, QMLLINT, QUICKSHELL_QML, QT_QML.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
tree=${OMARCHY_SHELL_TREE:-/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell}
qt=${QT_QML:-/nix/store/0jnxd2cwya279hyr7hmklbpdb0298lg1-qtdeclarative-6.12.0}
qs=${QUICKSHELL_QML:-/nix/store/cdgy4lw5j55scph4r8vqfgix46lisfbr-quickshell-0.3.2/lib/qt-6/qml}
w=$(mktemp -d); trap 'rm -rf "$w"' EXIT
mkdir "$w/qs"; ln -s "$tree/Commons" "$w/qs/Commons"; ln -s "$tree/Ui" "$w/qs/Ui"
"${QMLLINT:-$qt/bin/qmllint}" -I "$w" -I "$qs" -I "$qt/lib/qt-6/qml" "$here"/*.qml
