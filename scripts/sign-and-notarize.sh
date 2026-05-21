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
#   VMUX_BUILD_PROFILE      - "release" | "local" | "dev"; selects the
#                             code-signing identifier suffix for auxiliary
#                             binaries so the embedded LaunchAgent's
#                             identifier matches the plist Label (and
#                             macOS groups it under Vmux.app instead of
#                             showing a standalone "unidentified developer"
#                             row in Login Items).
#   VMUX_GIT_HASH           - required when VMUX_BUILD_PROFILE=local.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${APP_BUNDLE:-$ROOT/target/release/Vmux.app}"
ENTITLEMENTS="$ROOT/packaging/macos/Vmux.entitlements"
PROFILE="${VMUX_BUILD_PROFILE:-release}"

case "$PROFILE" in
    release)
        IDENT_SUFFIX=""
        ;;
    local)
        : "${VMUX_GIT_HASH:?VMUX_GIT_HASH must be set when VMUX_BUILD_PROFILE=local}"
        IDENT_SUFFIX=".$VMUX_GIT_HASH"
        ;;
    dev)
        IDENT_SUFFIX=".dev"
        ;;
    *)
        echo "Error: unknown VMUX_BUILD_PROFILE=$PROFILE (expected release|local|dev)" >&2
        exit 1
        ;;
esac

aux_identifier() {
    local name="$1"
    case "$name" in
        vmux_service) printf 'ai.vmux.service%s' "$IDENT_SUFFIX" ;;
        vmux)         printf 'ai.vmux.cli%s'     "$IDENT_SUFFIX" ;;
        *)            printf 'ai.vmux.%s%s' "$name" "$IDENT_SUFFIX" ;;
    esac
}

if [ ! -d "$APP_BUNDLE" ]; then
    echo "Error: $APP_BUNDLE not found. Run scripts/bundle-macos.sh first." >&2
    exit 1
fi

if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
    echo "Error: APPLE_SIGNING_IDENTITY not set." >&2
    echo "  Example: \"Developer ID Application: Your Name (XXXXXXXXXX)\"" >&2
    exit 1
fi

CODESIGN_KEYCHAIN_ARGS=()
if [ -n "${CODESIGN_KEYCHAIN:-}" ]; then
    CODESIGN_KEYCHAIN_ARGS=(--keychain "$CODESIGN_KEYCHAIN")
fi

echo "==> Signing $APP_BUNDLE"

# Sign the CEF framework and helper binaries first (inside-out signing)
# Find all Mach-O binaries and .dylib files in Frameworks
find "$APP_BUNDLE/Contents/Frameworks" -type f \( -name "*.dylib" -o -perm +111 \) | while read -r binary; do
    # Skip non-Mach-O files
    file "$binary" | grep -q "Mach-O" || continue
    echo "  Signing: ${binary#$APP_BUNDLE/}"
    codesign --force --verify --verbose \
        ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        "$binary"
done

# Sign the framework bundle itself
if [ -d "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework" ]; then
    echo "  Signing: Chromium Embedded Framework.framework"
    codesign --force --verify --verbose \
        ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework"
fi

# Sign all CEF Helper app bundles
find "$APP_BUNDLE/Contents/Frameworks" -name "*.app" -type d | while read -r helper; do
    echo "  Signing: ${helper#$APP_BUNDLE/}"
    codesign --force --verify --verbose \
        ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        --entitlements "$ENTITLEMENTS" \
        "$helper"
done

# Sign all auxiliary executables in Contents/MacOS (e.g. vmux CLI, vmux_service).
# Each gets an explicit --identifier in the ai.vmux.* namespace so macOS groups
# the LaunchAgent under Vmux.app in Login Items instead of as a standalone row.
find "$APP_BUNDLE/Contents/MacOS" -type f -perm +111 | while read -r binary; do
    file "$binary" | grep -q "Mach-O" || continue
    name="$(basename "$binary")"
    [ "$name" = "Vmux" ] && continue
    ident="$(aux_identifier "$name")"
    echo "  Signing: ${binary#$APP_BUNDLE/} (identifier=$ident)"
    codesign --force --verify --verbose \
        ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --identifier "$ident" \
        --options runtime \
        --entitlements "$ENTITLEMENTS" \
        "$binary"
done

# Sign the main app bundle
echo "  Signing: Vmux.app"
codesign --force --verify --verbose \
    ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} \
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
APP_BASENAME="$(basename "$APP_BUNDLE" .app)"
NOTARIZE_ZIP="$ROOT/target/release/${APP_BASENAME// /_}-notarize.zip"
rm -f "$NOTARIZE_ZIP"
ditto -c -k --keepParent "$APP_BUNDLE" "$NOTARIZE_ZIP"
ls -lh "$NOTARIZE_ZIP"

echo "==> Submitting for notarization (this may take several minutes)"
SUBMIT_OUTPUT="$(xcrun notarytool submit "$NOTARIZE_ZIP" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_APP_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait 2>&1)"
echo "$SUBMIT_OUTPUT"
SUBMIT_ID="$(echo "$SUBMIT_OUTPUT" | awk '/^  id:/ {print $2; exit}')"
if echo "$SUBMIT_OUTPUT" | grep -q "status: Invalid\|status: Rejected"; then
    echo "==> Notarization failed; fetching log for $SUBMIT_ID"
    xcrun notarytool log "$SUBMIT_ID" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_APP_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" || true
    exit 1
fi

echo "==> Stapling notarization ticket"
xcrun stapler staple "$APP_BUNDLE"

echo "==> Verifying notarization"
spctl --assess --type execute --verbose "$APP_BUNDLE"

rm -f "$NOTARIZE_ZIP"
echo "Done: $APP_BUNDLE is signed and notarized."
