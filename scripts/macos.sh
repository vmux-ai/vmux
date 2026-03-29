#!/usr/bin/env bash
# macOS: build and run vmux (development) or build release bundle and open the .app.
#
# Development (default): needs CEF under ~/.local/share and bevy_cef_debug_render_process
# in the framework Libraries folder (see README "First checkout — run for development").
#
# Usage:
#   ./scripts/macos.sh           # or: ./scripts/macos.sh dev
#   ./scripts/macos.sh bundle    # release .app + open (needs bevy_cef_render_process, bevy_cef_bundle_app)
#   ./scripts/macos.sh --help

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="${HOME}/.cargo/bin:${PATH}"
unset CEF_PATH

CEF_FRAMEWORK="${HOME}/.local/share/Chromium Embedded Framework.framework"
DEBUG_RENDER="${CEF_FRAMEWORK}/Libraries/bevy_cef_debug_render_process"
BUILD_DIR="${BUILD_DIR:-$ROOT/build}"
APP_BUNDLE="${APP_BUNDLE:-$BUILD_DIR/Vmux.app}"

usage() {
	cat <<'EOF'
Usage: macos.sh [command]

  dev       Build and run with CEF debug mode (cargo run -p vmux --features debug). Default.
  bundle    cargo build -p vmux --release, run bevy_cef_bundle_app, open Vmux.app
  help      Show this help

Environment (optional):
  BUILD_DIR     Where Vmux.app is written when bundling (default: <repo>/build)
  APP_BUNDLE  Full path to .app when opening after bundle (default: $BUILD_DIR/Vmux.app)
EOF
}

ensure_dev_prereqs() {
	if [[ ! -d "$CEF_FRAMEWORK" ]]; then
		echo "error: CEF framework not found:" >&2
		echo "  $CEF_FRAMEWORK" >&2
		echo >&2
		echo "Install CEF (one-time):" >&2
		echo '  cargo install export-cef-dir@145.6.1+145.0.28' >&2
		echo '  export-cef-dir --force "$HOME/.local/share"' >&2
		exit 1
	fi
	if [[ ! -f "$DEBUG_RENDER" ]]; then
		echo "error: macOS debug render helper not found:" >&2
		echo "  $DEBUG_RENDER" >&2
		echo >&2
		echo "Install and copy (one-time):" >&2
		echo "  cargo install bevy_cef_debug_render_process" >&2
		echo "  cp \"\$HOME/.cargo/bin/bevy_cef_debug_render_process\" \\" >&2
		echo "    \"$DEBUG_RENDER\"" >&2
		exit 1
	fi
}

cmd_dev() {
	ensure_dev_prereqs
	cd "$ROOT"
	echo "==> cargo run -p vmux --features debug"
	cargo run -p vmux --features debug
}

cmd_bundle() {
	cd "$ROOT"
	if [[ ! -x "$ROOT/scripts/bundle-macos.sh" ]]; then
		chmod +x "$ROOT/scripts/bundle-macos.sh"
	fi
	echo "==> bundle release app"
	"$ROOT/scripts/bundle-macos.sh"
	if [[ ! -d "$APP_BUNDLE" ]]; then
		echo "error: expected app bundle at $APP_BUNDLE" >&2
		exit 1
	fi
	echo "==> open $APP_BUNDLE"
	open "$APP_BUNDLE"
}

main() {
	case "${1:-dev}" in
	dev)
		cmd_dev
		;;
	bundle)
		cmd_bundle
		;;
	-h | --help | help)
		usage
		;;
	*)
		echo "unknown command: $1" >&2
		usage >&2
		exit 1
		;;
	esac
}

main "$@"
