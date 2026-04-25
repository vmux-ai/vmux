#!/usr/bin/env bash
set -euo pipefail

# Sign and notarize Vmux.app for macOS distribution.
#
# Required environment variables:
#   APPLE_SIGNING_IDENTITY  - "Developer ID Application: Name (TEAM_ID)"
#   APPLE_ID                - Apple ID email for notarytool
#   APPLE_APP_PASSWORD      - App-specific password
#   APPLE_TEAM_ID           - 10-character team identifier
#
# Optional:
#   APP_BUNDLE              - Path to .app (default: target/release/Vmux.app)
#   SKIP_NOTARIZE           - Set to "1" to skip notarization (local testing)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${APP_BUNDLE:-$ROOT/target/release/Vmux.app}"
ENTITLEMENTS="$ROOT/packaging/macos/Vmux.entitlements"

if [ ! -d "$APP_BUNDLE" ]; then
    echo "Error: $APP_BUNDLE not found. Run scripts/bundle-macos.sh first." >&2
    exit 1
fi

if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
    echo "Error: APPLE_SIGNING_IDENTITY not set." >&2
    echo "  Example: \"Developer ID Application: Your Name (XXXXXXXXXX)\"" >&2
    exit 1
fi

echo "==> Signing $APP_BUNDLE"

# Sign the CEF framework and helper binaries first (inside-out signing)
# Find all Mach-O binaries and .dylib files in Frameworks
find "$APP_BUNDLE/Contents/Frameworks" -type f \( -name "*.dylib" -o -perm +111 \) | while read -r binary; do
    # Skip non-Mach-O files
    file "$binary" | grep -q "Mach-O" || continue
    echo "  Signing: ${binary#$APP_BUNDLE/}"
    codesign --force --verify --verbose \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        "$binary"
done

# Sign the framework bundle itself
if [ -d "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework" ]; then
    echo "  Signing: Chromium Embedded Framework.framework"
    codesign --force --verify --verbose \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework"
fi

# Sign all CEF Helper app bundles
find "$APP_BUNDLE/Contents/Frameworks" -name "*.app" -type d | while read -r helper; do
    echo "  Signing: ${helper#$APP_BUNDLE/}"
    codesign --force --verify --verbose \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        --entitlements "$ENTITLEMENTS" \
        "$helper"
done

# Sign the main app bundle
echo "  Signing: Vmux.app"
codesign --force --verify --verbose \
    --sign "$APPLE_SIGNING_IDENTITY" \
    --options runtime \
    --entitlements "$ENTITLEMENTS" \
    "$APP_BUNDLE"

# Verify
echo "==> Verifying signature"
codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"

if [ "${SKIP_NOTARIZE:-}" = "1" ]; then
    echo "==> Skipping notarization (SKIP_NOTARIZE=1)"
    exit 0
fi

# Notarize
if [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_APP_PASSWORD:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ]; then
    echo "Error: APPLE_ID, APPLE_APP_PASSWORD, and APPLE_TEAM_ID must be set for notarization." >&2
    exit 1
fi

echo "==> Creating zip for notarization"
NOTARIZE_ZIP="$ROOT/target/release/Vmux-notarize.zip"
ditto -c -k --keepParent "$APP_BUNDLE" "$NOTARIZE_ZIP"

echo "==> Submitting for notarization (this may take several minutes)"
xcrun notarytool submit "$NOTARIZE_ZIP" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_APP_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait

echo "==> Stapling notarization ticket"
xcrun stapler staple "$APP_BUNDLE"

echo "==> Verifying notarization"
spctl --assess --type execute --verbose "$APP_BUNDLE"

rm -f "$NOTARIZE_ZIP"
echo "Done: $APP_BUNDLE is signed and notarized."
