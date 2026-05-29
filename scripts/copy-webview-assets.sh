#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${APP_BUNDLE:-${VMUX_APP_BUNDLE:-${ROOT}/target/release/Vmux.app}}"
WEBVIEW_ROOT="$APP_BUNDLE/Contents/Resources/webview-apps"

if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "copy-webview-assets: .app not found at $APP_BUNDLE" >&2
    exit 1
fi

copy_webview_app() {
    local host="$1"
    local src="$ROOT/crates/vmux_app/dist"
    local dest="$WEBVIEW_ROOT/$host"

    if [[ ! -f "$src/index.html" ]]; then
        echo "copy-webview-assets: missing $src/index.html" >&2
        exit 1
    fi

    mkdir -p "$dest"
    cp -R "$src/." "$dest/"
}

rm -rf "$WEBVIEW_ROOT"
mkdir -p "$WEBVIEW_ROOT"

copy_webview_app "layout"
copy_webview_app "command-bar"
copy_webview_app "terminal"
copy_webview_app "services"
copy_webview_app "history"
copy_webview_app "spaces"
copy_webview_app "settings"

echo "copy-webview-assets: copied webview assets to $WEBVIEW_ROOT"
