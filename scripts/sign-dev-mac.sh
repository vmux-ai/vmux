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
if ! codesign --verify --strict "$HELPER_BINARY" >/dev/null 2>&1; then
    sign_binary "$HELPER_BINARY" "$APP_IDENTIFIER.helper.$helper_basename"
fi
stamp="${APP_BINARY}.dev-signed"
current="${stamp}.tmp"
{
    printf '%s\n' "$APPLE_SIGNING_IDENTITY" "$APP_IDENTIFIER" "$CODESIGN_KEYCHAIN"
    shasum -a 256 "$APP_BINARY" "$ENTITLEMENTS"
} > "$current"
if [[ ! -f "$stamp" ]] || ! cmp -s "$current" "$stamp" \
    || ! codesign --verify --strict "$APP_BINARY" >/dev/null 2>&1; then
    sign_binary "$APP_BINARY" "$APP_IDENTIFIER"
    {
        printf '%s\n' "$APPLE_SIGNING_IDENTITY" "$APP_IDENTIFIER" "$CODESIGN_KEYCHAIN"
        shasum -a 256 "$APP_BINARY" "$ENTITLEMENTS"
    } > "$current"
    mv -f "$current" "$stamp"
else
    rm -f "$current"
fi

codesign --verify --strict --verbose=2 "$APP_BINARY"
