#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${APP_BUNDLE:-${VMUX_APP_BUNDLE:-${ROOT}/target/release/Vmux.app}}"
WEBVIEW_ROOT="$APP_BUNDLE/Contents/Resources/webview-apps"

if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "copy-webview-assets: .app not found at $APP_BUNDLE" >&2
    exit 1
fi

copy_shared_webview_app() {
    local src="${VMUX_WEB_BUNDLE_DIST:-$ROOT/crates/vmux_server/dist}"
    local dest="$WEBVIEW_ROOT/_shared"

    "$ROOT/scripts/verify-web-bundle.sh" release "$src"
    if [[ ! -f "$src/index.html" ]]; then
        echo "copy-webview-assets: missing $src/index.html" >&2
        exit 1
    fi

    mkdir -p "$dest"
    cp -R "$src/." "$dest/"
    "$ROOT/scripts/verify-web-bundle.sh" release "$dest"
}

rm -rf "$WEBVIEW_ROOT"
mkdir -p "$WEBVIEW_ROOT"

copy_shared_webview_app

echo "copy-webview-assets: copied webview assets to $WEBVIEW_ROOT"
