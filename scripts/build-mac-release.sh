#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
PROFILE="${1:-release}"

if [[ "${VMUX_BUILD_STORAGE_ROOT:-}" != "$ROOT" && -f "$ROOT/.env" ]]; then
    while IFS= read -r line || [[ -n "$line" ]]; do
        [[ -z "$line" || "$line" == \#* ]] && continue
        [[ "$line" == *=* ]] || continue
        key="${line%%=*}"
        value="${line#*=}"
        [[ "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
        export "$key=$value"
    done < "$ROOT/.env"
fi

source "$ROOT/scripts/cargo-target-paths.sh"
if [[ "${VMUX_BUILD_STORAGE_ROOT:-}" != "$ROOT" \
    || "${VMUX_BUILD_STORAGE_TARGET:-}" != "$(vmux_cargo_target_dir "$ROOT")" \
    || ! "${VMUX_TARGET_LOCK_OWNER_PID:-}" =~ ^[1-9][0-9]*$ ]] \
    || ! kill -0 "${VMUX_TARGET_LOCK_OWNER_PID:-0}" 2>/dev/null; then
    exec "$ROOT/scripts/with-build-storage.sh" "$PROFILE" "$0" "$@"
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

if [[ -n "${APPLE_CERTIFICATE:-}" || -n "${APPLE_CERTIFICATE_PASSWORD:-}" ]]; then
    : "${APPLE_CERTIFICATE:?missing APPLE_CERTIFICATE}"
    : "${APPLE_CERTIFICATE_PASSWORD:?missing APPLE_CERTIFICATE_PASSWORD}"
    : "${APPLE_SIGNING_IDENTITY:?missing APPLE_SIGNING_IDENTITY}"

    echo "==> Setting up ephemeral signing keychain"
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

source "$ROOT/scripts/cargo-target-paths.sh"
RELEASE_DIR="$(vmux_cargo_profile_dir "$ROOT" release)"
case "$PROFILE" in
    local) APP_BUNDLE="$RELEASE_DIR/Vmux ($(git -C "$ROOT" rev-parse --short=7 HEAD)).app" ;;
    release) APP_BUNDLE="$RELEASE_DIR/Vmux.app" ;;
esac

codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"
VERIFY_DIR="$(mktemp -d -t vmux-signature-check)"
if ! (cd "$VERIFY_DIR" && codesign --display --extract-certificates \
    "$APP_BUNDLE/Contents/MacOS/vmux" >/dev/null 2>&1); then
    rm -rf "$VERIFY_DIR"
    echo "Error: packaged Vault key broker is not certificate-signed." >&2
    exit 1
fi
if [[ ! -s "$VERIFY_DIR/codesign0" ]]; then
    rm -rf "$VERIFY_DIR"
    echo "Error: packaged Vault key broker has no signing certificate." >&2
    exit 1
fi
rm -rf "$VERIFY_DIR"
