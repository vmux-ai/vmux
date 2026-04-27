#!/usr/bin/env bash
set -euo pipefail

# Inject CEF framework into .app bundle via bevy_cef_bundle_app.
# Called by cargo-packager as before-each-package-command.
# Only runs when processing DMG format (app already built by then).

if [[ "${CARGO_PACKAGER_FORMAT:-}" != "dmg" ]]; then
    echo "inject-cef: skipping (format=${CARGO_PACKAGER_FORMAT:-unknown}, waiting for dmg)"
    exit 0
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="${HOME}/.cargo/bin:${PATH}"
# Wrong CEF_PATH breaks cef-dll-sys; default macOS layout is ~/.local/share.
unset CEF_PATH

APP_BUNDLE="${VMUX_APP_BUNDLE:-${ROOT}/target/release/Vmux.app}"
BUNDLE_ID_BASE="${VMUX_BUNDLE_ID:-ai.vmux.desktop}"
HELPER_BIN="${ROOT}/target/release/bevy_cef_debug_render_process"

if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "inject-cef: .app not found at $APP_BUNDLE, skipping"
    exit 0
fi

if [[ -d "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework" ]]; then
    echo "inject-cef: CEF already injected, skipping"
    exit 0
fi

if ! command -v bevy_cef_bundle_app >/dev/null 2>&1; then
    echo "inject-cef: bevy_cef_bundle_app not found. Install with: cargo install bevy_cef_bundle_app" >&2
    exit 1
fi

if [[ ! -f "$HELPER_BIN" ]]; then
    echo "inject-cef: helper binary not found at $HELPER_BIN" >&2
    echo "  Build it first: cargo build -p bevy_cef_debug_render_process --release" >&2
    exit 1
fi

echo "==> inject-cef: running bevy_cef_bundle_app"
bevy_cef_bundle_app --app "$APP_BUNDLE" --bundle-id-base "$BUNDLE_ID_BASE" --helper-bin "$HELPER_BIN"

# Copy app icon (cargo-packager handles this via icons config, but ensure it's there)
ICNS_SRC="$ROOT/packaging/macos/Vmux.icns"
if [[ -f "$ICNS_SRC" && ! -f "$APP_BUNDLE/Contents/Resources/Vmux.icns" ]]; then
    mkdir -p "$APP_BUNDLE/Contents/Resources"
    cp -f "$ICNS_SRC" "$APP_BUNDLE/Contents/Resources/Vmux.icns"
fi

echo "==> inject-cef: done"
