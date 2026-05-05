#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

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

CERT_FILE=""
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
    if [[ -n "$CERT_FILE" ]]; then
        rm -f "$CERT_FILE"
    fi
}
trap cleanup EXIT

if [[ -n "${APPLE_CERTIFICATE:-}" || -n "${APPLE_CERTIFICATE_PASSWORD:-}" ]]; then
    : "${APPLE_CERTIFICATE:?missing APPLE_CERTIFICATE}"
    : "${APPLE_CERTIFICATE_PASSWORD:?missing APPLE_CERTIFICATE_PASSWORD}"
    : "${APPLE_SIGNING_IDENTITY:?missing APPLE_SIGNING_IDENTITY}"

    CERT_FILE="$(mktemp /tmp/vmux-cert.XXXXXX.p12)"
    KEYCHAIN="$(mktemp -u /tmp/vmux-signing.XXXXXX.keychain-db)"
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
fi

cd "$ROOT"
env -u CEF_PATH cargo packager --release --formats app
CARGO_PACKAGER_FORMAT=dmg "$ROOT/scripts/inject-cef.sh"

APP_BUNDLE="$ROOT/target/release/Vmux.app" "$ROOT/scripts/sign-and-notarize.sh"
"$ROOT/scripts/create-dmg.sh"
