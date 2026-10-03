#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${APP_BUNDLE:-$ROOT/target/release/Vmux.app}"
ENTITLEMENTS="$ROOT/packaging/macos/Vmux.entitlements"
PROFILE="${VMUX_BUILD_PROFILE:-release}"

case "$PROFILE" in
    release)
        IDENT_SUFFIX=""
        ;;
    local)
        VMUX_GIT_HASH="${VMUX_GIT_HASH:-$(git -C "$ROOT" rev-parse --short=7 HEAD 2>/dev/null || true)}"
        : "${VMUX_GIT_HASH:?VMUX_GIT_HASH unset and git short hash unavailable}"
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
        "Vmux Service") printf 'ai.vmux.service%s' "$IDENT_SUFFIX" ;;
        vmux_service) printf 'ai.vmux.service%s' "$IDENT_SUFFIX" ;;
        vmux)         printf 'ai.vmux.cli'                         ;;
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

find "$APP_BUNDLE/Contents/Frameworks" -type f \( -name "*.dylib" -o -perm +111 \) | while read -r binary; do
    file "$binary" | grep -q "Mach-O" || continue
    echo "  Signing: ${binary#$APP_BUNDLE/}"
    codesign --force --verify --verbose \
        ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        "$binary"
done

if [ -d "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework" ]; then
    echo "  Signing: Chromium Embedded Framework.framework"
    codesign --force --verify --verbose \
        ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework"
fi

NESTED_APP_DIRS=("$APP_BUNDLE/Contents/Frameworks")
if [[ -d "$APP_BUNDLE/Contents/Library" ]]; then
    NESTED_APP_DIRS+=("$APP_BUNDLE/Contents/Library")
fi

find "${NESTED_APP_DIRS[@]}" -name "*.app" -type d | while read -r helper; do
    echo "  Signing: ${helper#$APP_BUNDLE/}"
    codesign --force --verify --verbose \
        ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        --entitlements "$ENTITLEMENTS" \
        "$helper"
done

find "$APP_BUNDLE/Contents/MacOS" -type f -perm +111 | while read -r binary; do
    file "$binary" | grep -q "Mach-O" || continue
    name="$(basename "$binary")"
    [ "$name" = "vmux_desktop" ] && continue
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

echo "  Signing: Vmux.app"
codesign --force --verify --verbose \
    ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} \
    --sign "$APPLE_SIGNING_IDENTITY" \
    --options runtime \
    --entitlements "$ENTITLEMENTS" \
    "$APP_BUNDLE"

echo "==> Verifying signature"
codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"

if [ "${SKIP_NOTARIZE:-}" = "1" ]; then
    echo "==> Skipping notarization (SKIP_NOTARIZE=1)"
    exit 0
fi

if [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_APP_PASSWORD:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ]; then
    echo "Error: APPLE_ID, APPLE_APP_PASSWORD, and APPLE_TEAM_ID must be set for notarization." >&2
    exit 1
fi

echo "==> Creating zip for notarization"
APP_BASENAME="$(basename "$APP_BUNDLE" .app)"
NOTARIZE_ZIP="$(dirname "$APP_BUNDLE")/${APP_BASENAME// /_}-notarize.zip"
rm -f "$NOTARIZE_ZIP"
ditto -c -k --keepParent "$APP_BUNDLE" "$NOTARIZE_ZIP"
ls -lh "$NOTARIZE_ZIP"

echo "==> Submitting for notarization (this may take several minutes)"
SUBMIT_OUTPUT=""
SUBMIT_STATUS=1
SUBMIT_ID=""
for attempt in 1 2 3; do
    set +e
    SUBMIT_OUTPUT="$(xcrun notarytool submit "$NOTARIZE_ZIP" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_APP_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" \
        --wait 2>&1)"
    SUBMIT_STATUS=$?
    set -e
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
    if [ "$SUBMIT_STATUS" -eq 0 ]; then
        break
    fi
    if [ -n "$SUBMIT_ID" ] || [ "$attempt" -eq 3 ]; then
        if [ -n "$SUBMIT_ID" ]; then
            echo "==> Notarization command failed; fetching log for $SUBMIT_ID"
            xcrun notarytool log "$SUBMIT_ID" \
                --apple-id "$APPLE_ID" \
                --password "$APPLE_APP_PASSWORD" \
                --team-id "$APPLE_TEAM_ID" || true
        fi
        exit "$SUBMIT_STATUS"
    fi
    echo "==> Notarization submission failed before receiving an ID; retrying ($attempt/3)"
    sleep $((attempt * 15))
done

echo "==> Stapling notarization ticket"
xcrun stapler staple "$APP_BUNDLE"

echo "==> Verifying notarization"
spctl --assess --type execute --verbose "$APP_BUNDLE"

rm -f "$NOTARIZE_ZIP"
echo "Done: $APP_BUNDLE is signed and notarized."
