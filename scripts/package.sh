#!/usr/bin/env bash
set -euo pipefail

# Profile-aware packaging for Vmux.
#
# Usage:
#   ./scripts/package.sh              # defaults to "local"
#   ./scripts/package.sh release
#   ./scripts/package.sh local
#
# Patches Cargo.toml packager metadata and Info.plist for the target profile,
# runs cargo packager, then restores the originals.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${1:-local}"

CARGO_TOML="$ROOT/crates/vmux_desktop/Cargo.toml"
INFO_PLIST="$ROOT/packaging/macos/Info.plist"

case "$PROFILE" in
    release)
        PRODUCT_NAME="Vmux"
        BUNDLE_ID="ai.vmux.desktop"
        ;;
    local)
        GIT_HASH=$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo "unknown")
        PRODUCT_NAME="Vmux ($GIT_HASH)"
        BUNDLE_ID="ai.vmux.desktop.local"
        ;;
    *)
        echo "Unknown profile: $PROFILE (expected: release, local)" >&2
        exit 1
        ;;
esac

echo "==> Packaging profile: $PROFILE"
echo "    Product name: $PRODUCT_NAME"
echo "    Bundle ID:    $BUNDLE_ID"

# Backup originals
cp "$CARGO_TOML" "$CARGO_TOML.bak"
cp "$INFO_PLIST" "$INFO_PLIST.bak"

restore() {
    mv "$CARGO_TOML.bak" "$CARGO_TOML"
    mv "$INFO_PLIST.bak" "$INFO_PLIST"
}
trap restore EXIT

# Patch Cargo.toml packager metadata
sed -i '' "s/^product-name = .*/product-name = \"$PRODUCT_NAME\"/" "$CARGO_TOML"
sed -i '' "s/^identifier = .*/identifier = \"$BUNDLE_ID\"/" "$CARGO_TOML"



# Patch Info.plist
sed -i '' "s|<string>ai\.vmux\.desktop</string>|<string>$BUNDLE_ID</string>|" "$INFO_PLIST"
# Update display name (the line after CFBundleDisplayName)
sed -i '' "/<key>CFBundleDisplayName<\/key>/{n;s|<string>.*</string>|<string>$PRODUCT_NAME</string>|;}" "$INFO_PLIST"
# Update bundle name (the line after CFBundleName)
sed -i '' "/<key>CFBundleName<\/key>/{n;s|<string>.*</string>|<string>$PRODUCT_NAME</string>|;}" "$INFO_PLIST"

# Export for inject-cef.sh
export VMUX_BUNDLE_ID="$BUNDLE_ID"
export VMUX_PROFILE="$PROFILE"

# The app bundle path uses the product name from packager metadata.
# cargo-packager always outputs to target/release/<product-name>.app
APP_NAME="$PRODUCT_NAME"
export VMUX_APP_BUNDLE="$ROOT/target/release/$APP_NAME.app"

echo "==> Running cargo packager"
cd "$ROOT"
env -u CEF_PATH VMUX_PROFILE="$PROFILE" cargo packager --release

# For local builds (app-only format), inject-cef.sh doesn't run via
# before-each-package-command because it only fires for DMG format
# (the .app doesn't exist yet during the app format pass).
# Run CEF injection manually after cargo-packager finishes.
if [[ "$PROFILE" == "local" && -d "$VMUX_APP_BUNDLE" ]]; then
    echo "==> Injecting CEF into .app (local build)"
    CARGO_PACKAGER_FORMAT=dmg bash "$ROOT/scripts/inject-cef.sh"
fi

echo "==> Packaging complete: $VMUX_APP_BUNDLE"
