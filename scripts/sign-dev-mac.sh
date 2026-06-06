#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BINARY="${APP_BINARY:-$ROOT/target/debug/vmux_desktop}"
HELPER_BINARY="${HELPER_BINARY:-$HOME/.local/share/Chromium Embedded Framework.framework/Libraries/bevy_cef_debug_render_process}"
ENTITLEMENTS="$ROOT/packaging/macos/VmuxDev.entitlements"
CODESIGN_KEYCHAIN="${CODESIGN_KEYCHAIN:-$(security default-keychain -d user | awk -F'"' '/"/ { print $2; exit }')}"
APP_IDENTIFIER="ai.vmux.desktop.dev"

if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    echo "Error: APPLE_SIGNING_IDENTITY not set." >&2
    exit 1
fi

CODESIGN_KEYCHAIN_ARGS=()
if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
    CODESIGN_KEYCHAIN_ARGS=(--keychain "$CODESIGN_KEYCHAIN")
fi

sign_binary() {
    local binary="$1"
    local identifier="$2"
    if [[ ! -f "$binary" ]]; then
        echo "Error: $binary not found." >&2
        exit 1
    fi
    if ! file "$binary" | grep -q "Mach-O"; then
        echo "Error: $binary is not a Mach-O binary." >&2
        exit 1
    fi
    codesign --force --verify --verbose \
        "${CODESIGN_KEYCHAIN_ARGS[@]}" \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --identifier "$identifier" \
        --options runtime \
        --entitlements "$ENTITLEMENTS" \
        "$binary"
}

helper_basename="$(printf '%s' "$(basename "$HELPER_BINARY")" | tr -c '[:alnum:]' '-')"
sign_binary "$HELPER_BINARY" "$APP_IDENTIFIER.helper.$helper_basename"
sign_binary "$APP_BINARY" "$APP_IDENTIFIER"

codesign --verify --strict --verbose=2 "$APP_BINARY"
