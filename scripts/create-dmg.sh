#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)}"
APP_BUNDLE="${APP_BUNDLE:-$ROOT/target/release/Vmux.app}"
DMG_PATH="${DMG_PATH:-$ROOT/target/release/Vmux_${VERSION}_aarch64.dmg}"

if [ ! -d "$APP_BUNDLE" ]; then
    echo "Error: $APP_BUNDLE not found." >&2
    exit 1
fi

CODESIGN_KEYCHAIN_ARGS=()
if [ -n "${CODESIGN_KEYCHAIN:-}" ]; then
    CODESIGN_KEYCHAIN_ARGS=(--keychain "$CODESIGN_KEYCHAIN")
fi

detach_stale_vmux_images() {
    hdiutil info | awk '
        /^image-path[[:space:]]*:/ {
            image = $0
            sub(/^image-path[[:space:]]*:[[:space:]]*/, "", image)
            next
        }
        /^\/dev\/disk/ && image ~ /\/Vmux_[^\/]*_rw\.dmg$/ {
            print $1
        }
    ' | while read -r device; do
        echo "Detaching stale Vmux disk image: $device"
        if ! hdiutil detach "$device"; then
            hdiutil detach -force "$device" || echo "Warning: failed to detach $device" >&2
        fi
    done
}

rm -f "$ROOT"/target/release/Vmux_*.dmg
detach_stale_vmux_images

for attempt in 1 2 3; do
    if hdiutil create -volname "Vmux" \
        -srcfolder "$APP_BUNDLE" \
        -ov -format UDZO \
        "$DMG_PATH"; then
        break
    fi
    if [ "$attempt" = "3" ]; then
        exit 1
    fi
    sleep "$attempt"
    detach_stale_vmux_images
done

if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
    codesign --force "${CODESIGN_KEYCHAIN_ARGS[@]}" --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$DMG_PATH"
fi

echo "Done: $DMG_PATH"
