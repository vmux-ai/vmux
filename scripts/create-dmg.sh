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

rm -f "$ROOT"/target/release/Vmux_*.dmg
hdiutil create -volname "Vmux" \
    -srcfolder "$APP_BUNDLE" \
    -ov -format UDZO \
    "$DMG_PATH"

if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
    codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$DMG_PATH"
fi

echo "Done: $DMG_PATH"
