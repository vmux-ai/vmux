#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${1:-local}"
export PATH="${HOME}/.cargo/bin:${PATH}"
source "$ROOT/scripts/cargo-target-paths.sh"

CARGO_TOML="$ROOT/crates/app/vmux_desktop/Cargo.toml"
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

[[ -f "$CARGO_TOML.bak" ]] || cp "$CARGO_TOML" "$CARGO_TOML.bak"
[[ -f "$INFO_PLIST.bak" ]] || cp "$INFO_PLIST" "$INFO_PLIST.bak"

restore() {
    [[ -f "$CARGO_TOML.bak" ]] && mv -f "$CARGO_TOML.bak" "$CARGO_TOML"
    [[ -f "$INFO_PLIST.bak" ]] && mv -f "$INFO_PLIST.bak" "$INFO_PLIST"
    return 0
}
trap restore EXIT

sed -i '' "s/^product-name = .*/product-name = \"$PRODUCT_NAME\"/" "$CARGO_TOML"
sed -i '' "s/^identifier = .*/identifier = \"$BUNDLE_ID\"/" "$CARGO_TOML"



sed -i '' "s|<string>ai\.vmux\.desktop</string>|<string>$BUNDLE_ID</string>|" "$INFO_PLIST"
sed -i '' "/<key>CFBundleDisplayName<\/key>/{n;s|<string>.*</string>|<string>$PRODUCT_NAME</string>|;}" "$INFO_PLIST"
sed -i '' "/<key>CFBundleName<\/key>/{n;s|<string>.*</string>|<string>$PRODUCT_NAME</string>|;}" "$INFO_PLIST"

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

if [[ "$PROFILE" == "local" && -d "$VMUX_APP_BUNDLE" ]]; then
    "$ROOT/scripts/build-title.sh" local "$ROOT" "$VMUX_APP_BUNDLE/Contents/Info.plist" >/dev/null
    echo "==> Injecting CEF into .app (local build)"
    CARGO_PACKAGER_FORMAT=dmg bash "$ROOT/scripts/inject-cef.sh"

    echo "==> Embedding launchd plist (local build)"
    VMUX_GIT_HASH="$SHA" "$ROOT/scripts/embed-launch-agent-plist.sh"

    echo "==> Signing + notarizing local build"
    APP_BUNDLE="$VMUX_APP_BUNDLE" VMUX_GIT_HASH="$SHA" "$ROOT/scripts/sign-and-notarize.sh"
fi

echo "==> Packaging complete: $VMUX_APP_BUNDLE"
