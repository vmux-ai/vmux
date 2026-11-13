#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/product-host.sh"
vmux_host_read "$ROOT"
APP_BINARY="${APP_BINARY:-$ROOT/target/debug/vmux_desktop}"
HELPER_BINARY="${HELPER_BINARY:-$HOME/.local/share/Chromium Embedded Framework.framework/Libraries/bevy_cef_debug_render_process}"
ENTITLEMENTS="$VMUX_HOST_DEV_ENTITLEMENTS"
CODESIGN_KEYCHAIN="${CODESIGN_KEYCHAIN:-$(security default-keychain -d user | awk -F'"' '/"/ { print $2; exit }')}"
APP_IDENTIFIER="$VMUX_HOST_BUNDLE_ID.dev"

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

sign_if_needed() {
    local binary="$1"
    local identifier="$2"
    local stamp="${binary}.dev-signed"
    local current="${stamp}.tmp"
    {
        printf '%s\n' "$APPLE_SIGNING_IDENTITY" "$identifier" "$CODESIGN_KEYCHAIN"
        shasum -a 256 "$binary" "$ENTITLEMENTS"
    } > "$current"
    if [[ ! -f "$stamp" ]] || ! cmp -s "$current" "$stamp" \
        || ! codesign --verify --strict "$binary" >/dev/null 2>&1; then
        sign_binary "$binary" "$identifier"
        {
            printf '%s\n' "$APPLE_SIGNING_IDENTITY" "$identifier" "$CODESIGN_KEYCHAIN"
            shasum -a 256 "$binary" "$ENTITLEMENTS"
        } > "$current"
        mv -f "$current" "$stamp"
    else
        rm -f "$current"
    fi
}

helper_basename="$(printf '%s' "$(basename "$HELPER_BINARY")" | tr -c '[:alnum:]' '-')"
sign_if_needed "$HELPER_BINARY" "$APP_IDENTIFIER.helper.$helper_basename"
sign_if_needed "$APP_BINARY" "$APP_IDENTIFIER"

codesign --verify --strict --verbose=2 "$APP_BINARY"
