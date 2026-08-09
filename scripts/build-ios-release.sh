#!/usr/bin/env bash
set -euo pipefail

# Build, sign and package Vmux for iOS as an App Store .ipa, optionally uploading it to
# TestFlight. Imports the distribution certificate from APPLE_IOS_CERTIFICATE /
# APPLE_IOS_CERTIFICATE_PASSWORD (or `.env`) into a temporary keychain so signing works the same
# way locally and in CI, mirroring scripts/build-mac-release.sh.
#
# Required:
#   APPLE_TEAM_ID                     ten-character team id
#   APPLE_IOS_SIGNING_IDENTITY        e.g. "Apple Distribution: Name (XXXXXXXXXX)"
#   APPLE_IOS_PROVISIONING_PROFILE    base64 of the .mobileprovision for ai.vmux.mobile
# Required unless the identity is already in the login keychain:
#   APPLE_IOS_CERTIFICATE             base64 of the distribution .p12
#   APPLE_IOS_CERTIFICATE_PASSWORD    that .p12's export password
# Required to upload (skip with SKIP_UPLOAD=1):
#   APPLE_ID, APPLE_APP_PASSWORD
# Optional:
#   VMUX_IOS_BUILD_NUMBER             CFBundleVersion; CI passes the run number
#   SKIP_UPLOAD=1                     build and sign only
#
# Usage: ./scripts/build-ios-release.sh

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/cargo-target-paths.sh"

if [[ -f "$ROOT/.env" ]]; then
    while IFS= read -r line || [[ -n "$line" ]]; do
        [[ -z "$line" || "$line" == \#* ]] && continue
        [[ "$line" == *=* ]] || continue
        key="${line%%=*}"
        value="${line#*=}"
        [[ "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
        export "$key=$value"
    done < "$ROOT/.env"
fi

: "${APPLE_TEAM_ID:?missing APPLE_TEAM_ID}"
: "${APPLE_IOS_SIGNING_IDENTITY:?missing APPLE_IOS_SIGNING_IDENTITY}"
: "${APPLE_IOS_PROVISIONING_PROFILE:?missing APPLE_IOS_PROVISIONING_PROFILE}"

DX_BIN="${DX_BIN:-$(command -v dx || echo "$HOME/.cargo/bin/dx")}"
TARGET_TRIPLE="aarch64-apple-ios"
BUNDLE_ID="ai.vmux.mobile"

TMP_DIR=""
KEYCHAIN=""
ORIGINAL_KEYCHAINS=()

while IFS= read -r keychain; do
    keychain="${keychain#"${keychain%%[![:space:]]*}"}"
    keychain="${keychain#\"}"
    keychain="${keychain%\"}"
    [[ -n "$keychain" ]] && ORIGINAL_KEYCHAINS+=("$keychain")
done < <(security list-keychains -d user)

cleanup() {
    if [[ "${#ORIGINAL_KEYCHAINS[@]}" -gt 0 ]]; then
        security list-keychains -d user -s "${ORIGINAL_KEYCHAINS[@]}" >/dev/null 2>&1 || true
    fi
    if [[ -n "$KEYCHAIN" ]]; then
        security delete-keychain "$KEYCHAIN" >/dev/null 2>&1 || true
    fi
    if [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]]; then
        rm -rf "$TMP_DIR"
    fi
}
trap cleanup EXIT

# BSD mktemp only substitutes X's at the end of the template, so use -t and let it name the dir.
TMP_DIR="$(mktemp -d -t vmux-ios-build)"

if [[ -n "${APPLE_IOS_CERTIFICATE:-}" || -n "${APPLE_IOS_CERTIFICATE_PASSWORD:-}" ]]; then
    : "${APPLE_IOS_CERTIFICATE:?missing APPLE_IOS_CERTIFICATE}"
    : "${APPLE_IOS_CERTIFICATE_PASSWORD:?missing APPLE_IOS_CERTIFICATE_PASSWORD}"

    echo "==> Setting up ephemeral signing keychain"
    CERT_FILE="$TMP_DIR/cert.p12"
    KEYCHAIN="$TMP_DIR/signing.keychain-db"
    KEYCHAIN_PASSWORD="$(uuidgen)"

    echo "$APPLE_IOS_CERTIFICATE" | base64 --decode > "$CERT_FILE"
    security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
    security set-keychain-settings -lut 21600 "$KEYCHAIN"
    security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
    if ! security import "$CERT_FILE" -P "$APPLE_IOS_CERTIFICATE_PASSWORD" -A -f pkcs12 -k "$KEYCHAIN"; then
        echo "Error: failed to import APPLE_IOS_CERTIFICATE. Check APPLE_IOS_CERTIFICATE_PASSWORD matches the .p12 export password." >&2
        exit 1
    fi
    security set-key-partition-list -S apple-tool:,apple: -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
    security list-keychains -d user -s "$KEYCHAIN" "${ORIGINAL_KEYCHAINS[@]}"
    if ! security find-identity -v -p codesigning "$KEYCHAIN" | grep -Fq "\"$APPLE_IOS_SIGNING_IDENTITY\""; then
        echo "Error: APPLE_IOS_SIGNING_IDENTITY does not match an imported codesigning identity." >&2
        echo "       A Developer ID certificate cannot sign for the App Store; this must be an Apple Distribution certificate." >&2
        exit 1
    fi
else
    echo "==> APPLE_IOS_CERTIFICATE not set; falling back to login keychain"
fi

CODESIGN_KEYCHAIN_ARGS=()
if [[ -n "$KEYCHAIN" ]]; then
    CODESIGN_KEYCHAIN_ARGS=(--keychain "$KEYCHAIN")
fi

# dx auto-provisioning only ever matches "Apple Development:" identities, so the profile is
# installed and the signing identity passed explicitly instead of letting dx guess.
PROFILE_FILE="$TMP_DIR/profile.mobileprovision"
echo "$APPLE_IOS_PROVISIONING_PROFILE" | base64 --decode > "$PROFILE_FILE"

PROFILE_BUNDLE_ID="$(security cms -D -i "$PROFILE_FILE" 2>/dev/null \
    | plutil -extract Entitlements.application-identifier raw - 2>/dev/null || true)"
if [[ "$PROFILE_BUNDLE_ID" != "$APPLE_TEAM_ID.$BUNDLE_ID" ]]; then
    echo "Error: provisioning profile is for '${PROFILE_BUNDLE_ID:-unknown}', expected '$APPLE_TEAM_ID.$BUNDLE_ID'." >&2
    exit 1
fi

echo "==> Building $TARGET_TRIPLE (release)"
cd "$ROOT"
"$DX_BIN" build --platform ios --release -p vmux_mobile --target "$TARGET_TRIPLE"

TARGET_DIR="$(vmux_cargo_target_dir "$ROOT")"
APP_BUNDLE="$TARGET_DIR/dx/vmux_mobile/release/ios/VmuxMobile.app"
if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "Error: expected a bundle at $APP_BUNDLE after the build." >&2
    exit 1
fi

echo "==> Injecting resources"
# Must happen between build and signing: the icons, launch screen and privacy manifest are part
# of what gets sealed. `dx bundle` would rebuild and undo this, which is why the .ipa is
# assembled below rather than by dx.
VMUX_IOS_PROFILE=release "$ROOT/scripts/inject-ios-resources.sh" "$APP_BUNDLE"

cp -f "$PROFILE_FILE" "$APP_BUNDLE/embedded.mobileprovision"

ENTITLEMENTS="$TMP_DIR/Vmux.entitlements"
sed -e "s|{{TEAM_ID}}|$APPLE_TEAM_ID|g" "$ROOT/packaging/ios/Vmux.entitlements" > "$ENTITLEMENTS"

echo "==> Signing"
codesign --force --timestamp --options runtime \
    --entitlements "$ENTITLEMENTS" \
    --sign "$APPLE_IOS_SIGNING_IDENTITY" \
    ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} \
    "$APP_BUNDLE"

codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"

echo "==> Packaging .ipa"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
OUTPUT_DIR="$TARGET_DIR/dx/vmux_mobile/release/ios/ipa"
IPA="$OUTPUT_DIR/Vmux_${VERSION}_aarch64.ipa"
mkdir -p "$OUTPUT_DIR"
rm -f "$IPA"

PAYLOAD="$TMP_DIR/Payload"
mkdir -p "$PAYLOAD"
cp -R "$APP_BUNDLE" "$PAYLOAD/"
(cd "$TMP_DIR" && ditto -c -k --sequesterRsrc --keepParent Payload "$IPA")

echo "==> Built $IPA"

if [[ "${SKIP_UPLOAD:-}" == "1" ]]; then
    echo "==> SKIP_UPLOAD=1, not uploading"
    exit 0
fi

: "${APPLE_ID:?missing APPLE_ID}"
: "${APPLE_APP_PASSWORD:?missing APPLE_APP_PASSWORD}"

echo "==> Uploading to App Store Connect"
xcrun altool --upload-app -f "$IPA" -t ios \
    -u "$APPLE_ID" -p "$APPLE_APP_PASSWORD" \
    --team-id "$APPLE_TEAM_ID"

echo "==> Uploaded. It appears in TestFlight once Apple finishes processing."
