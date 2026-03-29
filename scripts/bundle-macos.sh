#!/usr/bin/env bash
set -euo pipefail

# Build release vmux and bundle CEF into Vmux.app using bevy_cef_bundle_app.
# Prerequisites: export-cef-dir (see README), cargo-installed bevy_cef_render_process + bevy_cef_bundle_app.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="${HOME}/.cargo/bin:${PATH}"
# Wrong CEF_PATH (e.g. Windows-style .../cef) breaks cef-dll-sys; default macOS layout is ~/.local/share.
unset CEF_PATH

BUILD_DIR="${BUILD_DIR:-$ROOT/build}"
APP_NAME="${APP_NAME:-Vmux}"
APP_BUNDLE="${APP_BUNDLE:-$BUILD_DIR/${APP_NAME}.app}"
BUNDLE_ID_BASE="${BUNDLE_ID_BASE:-com.yourorg.vmux}"
PLIST_SRC="${PLIST_SRC:-$ROOT/packaging/macos/Info.plist}"

cd "$ROOT"

echo "==> cargo build -p vmux --release"
cargo build -p vmux --release

mkdir -p "$APP_BUNDLE/Contents/MacOS"
cp -f "$ROOT/target/release/vmux" "$APP_BUNDLE/Contents/MacOS/vmux"
chmod +x "$APP_BUNDLE/Contents/MacOS/vmux"
cp -f "$PLIST_SRC" "$APP_BUNDLE/Contents/Info.plist"

if ! command -v bevy_cef_bundle_app >/dev/null 2>&1; then
	echo "bevy_cef_bundle_app not found. Install with:" >&2
	echo "  cargo install bevy_cef_bundle_app" >&2
	exit 1
fi

if ! command -v bevy_cef_render_process >/dev/null 2>&1; then
	echo "bevy_cef_render_process not in PATH (~/.cargo/bin). Install with:" >&2
	echo "  cargo install bevy_cef_render_process" >&2
	exit 1
fi

echo "==> bevy_cef_bundle_app"
bevy_cef_bundle_app --app "$APP_BUNDLE" --bundle-id-base "$BUNDLE_ID_BASE"

echo "Done: $APP_BUNDLE"
echo "  open \"$APP_BUNDLE\""
