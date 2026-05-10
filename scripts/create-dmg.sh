#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)}"
APP_BUNDLE="${APP_BUNDLE:-$ROOT/target/release/Vmux.app}"
DEFAULT_NAME="$(basename "$APP_BUNDLE" .app)"
DMG_PATH="${DMG_PATH:-$ROOT/target/release/${DEFAULT_NAME// /_}_${VERSION}_aarch64.dmg}"
VOL_NAME="${VOL_NAME:-$DEFAULT_NAME}"

if [ ! -d "$APP_BUNDLE" ]; then
    echo "Error: $APP_BUNDLE not found." >&2
    exit 1
fi

CODESIGN_KEYCHAIN_ARGS=()
if [ -n "${CODESIGN_KEYCHAIN:-}" ]; then
    CODESIGN_KEYCHAIN_ARGS=(--keychain "$CODESIGN_KEYCHAIN")
fi

if [ -d "/Volumes/$VOL_NAME" ]; then
    DEV_TO_DETACH="$(hdiutil info | awk -v vol="$VOL_NAME" '
        /^\/dev\/disk/ && $0 ~ ("Apple_HFS[[:space:]]+/Volumes/" vol "($|[[:space:]])") {
            print $1
        }
    ')"
    for device in $DEV_TO_DETACH; do
        echo "Detaching stale disk image: $device"
        hdiutil detach "$device" 2>/dev/null || hdiutil detach -force "$device" || true
    done
fi

rm -f "$DMG_PATH"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP_BUNDLE" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

hdiutil create \
    -volname "$VOL_NAME" \
    -srcfolder "$STAGE" \
    -fs HFS+ \
    -format UDZO \
    -imagekey zlib-level=9 \
    -ov \
    "$DMG_PATH"

if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
    codesign --force "${CODESIGN_KEYCHAIN_ARGS[@]}" --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$DMG_PATH"
fi

echo "Done: $DMG_PATH"
