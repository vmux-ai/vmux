#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${1:-}"
MANIFEST="${HOME}/Library/Application Support/Vmux/vault/vault.ron"
ENTITLEMENTS="$ROOT/packaging/macos/Vmux.entitlements"

if [[ -z "$APP_BUNDLE" || ! -x "$APP_BUNDLE/Contents/MacOS/vmux" ]]; then
    echo "Usage: $0 '/path/to/current/Vmux.app'" >&2
    exit 1
fi
if [[ ! -f "$MANIFEST" ]]; then
    echo "No local Vault manifest found." >&2
    exit 1
fi

VAULT_ID="$(awk -F'"' '/vault_id:/ { print $2; exit }' "$MANIFEST")"
if [[ ! "$VAULT_ID" =~ ^[0-9a-fA-F]{32}$ ]]; then
    echo "Invalid local Vault identifier." >&2
    exit 1
fi

IDENTITY="$($ROOT/scripts/ensure-local-codesign-identity.sh)"
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TEMP_DIR"' EXIT
cp "$APP_BUNDLE/Contents/MacOS/vmux" "$TEMP_DIR/vmux"
chmod +x "$TEMP_DIR/vmux"

while IFS= read -r OLD_APP; do
    BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$OLD_APP/Contents/Info.plist" 2>/dev/null || true)"
    [[ "$BUNDLE_ID" == ai.vmux.desktop.* ]] || continue
    cp "$TEMP_DIR/vmux" "$TEMP_DIR/vmux_desktop"
    codesign --force --sign "$IDENTITY" --identifier "$BUNDLE_ID" --options runtime \
        --entitlements "$ENTITLEMENTS" "$TEMP_DIR/vmux_desktop" >/dev/null
    set +e
    "$TEMP_DIR/vmux_desktop" vault-key migrate --vault-id "$VAULT_ID"
    STATUS=$?
    set -e
    if [[ $STATUS -eq 0 ]]; then
        echo "Migrated Vault key from $BUNDLE_ID."
        exit 0
    fi
    if [[ $STATUS -ne 2 ]]; then
        exit "$STATUS"
    fi
done < <(find "$ROOT/target/release" -maxdepth 1 -type d -name 'Vmux (*.app)' -print | sort -r)

echo "No matching local Vmux build could access this Vault key." >&2
exit 2
