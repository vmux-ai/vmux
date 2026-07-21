#!/usr/bin/env bash
set -euo pipefail

# Build Vmux for macOS. Imports the signing certificate from the
# APPLE_CERTIFICATE / APPLE_CERTIFICATE_PASSWORD env vars (or `.env`)
# into a temporary keychain so signing works the same way locally and
# in CI. Without those env vars, falls back to the user's login keychain.
#
# Usage:
#   ./scripts/build-mac.sh           # release (default)
#   ./scripts/build-mac.sh release
#   ./scripts/build-mac.sh local

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${1:-release}"

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

if [[ "$PROFILE" == "local" ]]; then
    export SKIP_NOTARIZE="${SKIP_NOTARIZE:-1}"
    if [[ -z "${APPLE_CERTIFICATE:-}" \
        && -z "${APPLE_CERTIFICATE_PASSWORD:-}" \
        && -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
        APPLE_SIGNING_IDENTITY="$("$ROOT/scripts/ensure-local-codesign-identity.sh")"
        export APPLE_SIGNING_IDENTITY
    fi
fi

TMP_DIR=""
KEYCHAIN=""
ORIGINAL_KEYCHAINS=()

while IFS= read -r keychain; do
    keychain="${keychain#\"}"
    keychain="${keychain%\"}"
    ORIGINAL_KEYCHAINS+=("$keychain")
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

if [[ -n "${APPLE_CERTIFICATE:-}" || -n "${APPLE_CERTIFICATE_PASSWORD:-}" ]]; then
    : "${APPLE_CERTIFICATE:?missing APPLE_CERTIFICATE}"
    : "${APPLE_CERTIFICATE_PASSWORD:?missing APPLE_CERTIFICATE_PASSWORD}"
    : "${APPLE_SIGNING_IDENTITY:?missing APPLE_SIGNING_IDENTITY}"

    echo "==> Setting up ephemeral signing keychain"
    # Use a temp directory; BSD mktemp on macOS doesn't substitute X's that
    # aren't at the end of the template, which causes literal-name clashes.
    TMP_DIR="$(mktemp -d -t vmux-build)"
    CERT_FILE="$TMP_DIR/cert.p12"
    KEYCHAIN="$TMP_DIR/signing.keychain-db"
    KEYCHAIN_PASSWORD="$(uuidgen)"

    echo "$APPLE_CERTIFICATE" | base64 --decode > "$CERT_FILE"
    security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
    security set-keychain-settings -lut 21600 "$KEYCHAIN"
    security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
    if ! security import "$CERT_FILE" -P "$APPLE_CERTIFICATE_PASSWORD" -A -f pkcs12 -k "$KEYCHAIN"; then
        echo "Error: failed to import APPLE_CERTIFICATE. Check APPLE_CERTIFICATE_PASSWORD matches the .p12 export password." >&2
        exit 1
    fi
    security set-key-partition-list -S apple-tool:,apple: -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
    security list-keychains -d user -s "$KEYCHAIN" "${ORIGINAL_KEYCHAINS[@]}"
    security find-identity -v -p codesigning "$KEYCHAIN"
    if ! security find-identity -v -p codesigning "$KEYCHAIN" | grep -Fq "\"$APPLE_SIGNING_IDENTITY\""; then
        echo "Error: APPLE_SIGNING_IDENTITY does not match an imported codesigning identity." >&2
        exit 1
    fi
    export CODESIGN_KEYCHAIN="$KEYCHAIN"
else
    echo "==> APPLE_CERTIFICATE not set; falling back to login keychain"
    echo "    (set APPLE_CERTIFICATE + APPLE_CERTIFICATE_PASSWORD in .env to import"
    echo "     the cert into a temp keychain like CI does)"
fi

cd "$ROOT"
"$ROOT/scripts/package.sh" "$PROFILE"
