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
export PATH="${HOME}/.cargo/bin:${PATH}"
source "$ROOT/scripts/cargo-target-paths.sh"

CARGO_TOML="$ROOT/crates/vmux_desktop/Cargo.toml"
INFO_PLIST="$ROOT/packaging/macos/Info.plist"

case "$PROFILE" in
    release)
        PRODUCT_NAME="Vmux"
        BUNDLE_ID="ai.vmux.desktop"
        ;;
    local)
        SHA="$(git -C "$ROOT" rev-parse --short=7 HEAD)"
        PRODUCT_NAME="Vmux ($SHA)"
        BUNDLE_ID="ai.vmux.desktop.$SHA"
        ;;
    *)
        echo "Unknown profile: $PROFILE (expected: release, local)" >&2
        exit 1
        ;;
esac

echo "==> Packaging profile: $PROFILE"
echo "    Product name: $PRODUCT_NAME"
echo "    Bundle ID:    $BUNDLE_ID"

# Backup originals. Skip if a .bak already exists from a crashed earlier
# run -- overwriting it would clobber the original with the patched state
# and permanently corrupt the working tree.
[[ -f "$CARGO_TOML.bak" ]] || cp "$CARGO_TOML" "$CARGO_TOML.bak"
[[ -f "$INFO_PLIST.bak" ]] || cp "$INFO_PLIST" "$INFO_PLIST.bak"

restore() {
    [[ -f "$CARGO_TOML.bak" ]] && mv -f "$CARGO_TOML.bak" "$CARGO_TOML"
    [[ -f "$INFO_PLIST.bak" ]] && mv -f "$INFO_PLIST.bak" "$INFO_PLIST"
    return 0
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
export VMUX_BUILD_PROFILE="$PROFILE"

APP_NAME="$PRODUCT_NAME"
export VMUX_CARGO_RELEASE_DIR="$(vmux_cargo_profile_dir "$ROOT" release)"
export VMUX_APP_BUNDLE="$VMUX_CARGO_RELEASE_DIR/$APP_NAME.app"

echo "==> Running cargo packager"
cd "$ROOT"
packager_args=(packager --release)
if [[ -n "${CARGO_BUILD_TARGET:-}" ]]; then
    packager_args+=(--target "$CARGO_BUILD_TARGET")
fi
if [[ "$PROFILE" == "local" ]]; then
    packager_args+=(--formats app)
fi
VMUX_BUILD_PROFILE="$PROFILE" "$ROOT/scripts/cargo-with-cef-cache.sh" "${packager_args[@]}"

# inject-cef only meaningfully runs in the dmg-format pass (the .app
# doesn't exist during the app-format pass). For local app-only builds,
# run it manually here so the freshly-built .app gets CEF + webview assets,
# then sign + notarize using the same identity as a release build.
if [[ "$PROFILE" == "local" && -d "$VMUX_APP_BUNDLE" ]]; then
    echo "==> Injecting CEF into .app (local build)"
    CARGO_PACKAGER_FORMAT=dmg bash "$ROOT/scripts/inject-cef.sh"

    echo "==> Embedding launchd plist (local build)"
    VMUX_GIT_HASH="$SHA" "$ROOT/scripts/embed-launch-agent-plist.sh"

    echo "==> Signing + notarizing local build"
    APP_BUNDLE="$VMUX_APP_BUNDLE" VMUX_GIT_HASH="$SHA" "$ROOT/scripts/sign-and-notarize.sh"
fi

echo "==> Packaging complete: $VMUX_APP_BUNDLE"
