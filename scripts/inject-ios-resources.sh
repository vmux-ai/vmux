#!/usr/bin/env bash
set -euo pipefail

# Build and copy hand-authored resources into a built iOS .app, and stamp its version keys.
#
# dx never calls copy_resources() for the iOS bundle, so `[ios] resources` in Dioxus.toml is a
# no-op, and dx has no iOS icon pipeline at all — no actool, no Assets.car, no launch screen.
# Everything the bundle needs beyond the executable is produced here, after `dx build` and
# before signing.
#
# packaging/ios/Info.plist replaces dx's template wholesale, which means the version keys are no
# longer derived from Cargo.toml. They are stamped onto the built copy here so the checked-in
# file cannot drift from the workspace version.
#
# Usage: scripts/inject-ios-resources.sh [path/to/App.app]
#        VMUX_IOS_PROFILE=release VMUX_IOS_BUILD_NUMBER=42 scripts/inject-ios-resources.sh

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/cargo-target-paths.sh"

PROFILE="${VMUX_IOS_PROFILE:-debug}"
case "$PROFILE" in
    debug | release) ;;
    *)
        echo "Error: unknown VMUX_IOS_PROFILE=$PROFILE (expected debug|release)" >&2
        exit 1
        ;;
esac

if ! xcrun --find actool >/dev/null 2>&1 || ! xcrun --find ibtool >/dev/null 2>&1; then
    echo "Error: actool/ibtool not found. Install Xcode (not just the Command Line Tools)." >&2
    exit 1
fi

TARGET_DIR="$(vmux_cargo_target_dir "$ROOT")"
DEFAULT_BUNDLE="$TARGET_DIR/dx/vmux_mobile/$PROFILE/ios/VmuxMobile.app"
APP_BUNDLE="${1:-${VMUX_IOS_APP_BUNDLE:-$DEFAULT_BUNDLE}}"

if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "Error: no .app at $APP_BUNDLE. Run 'make mobile-ios' first." >&2
    exit 1
fi

PLIST="$APP_BUNDLE/Info.plist"
if [[ ! -f "$PLIST" ]]; then
    echo "Error: no Info.plist in $APP_BUNDLE." >&2
    exit 1
fi

PACKAGING="$ROOT/packaging/ios"
DEPLOYMENT_TARGET="15.0"

STAGE="$(mktemp -d -t vmux-ios-assets)"
cleanup() { rm -rf "$STAGE"; }
trap cleanup EXIT

# --target-device iphone keeps this in step with UIDeviceFamily = [1] in Info.plist. The
# universal default also emits an ~ipad icon and doubles the size of Assets.car.
xcrun actool "$PACKAGING/Assets.xcassets" \
    --compile "$STAGE" \
    --platform iphoneos \
    --target-device iphone \
    --minimum-deployment-target "$DEPLOYMENT_TARGET" \
    --app-icon AppIcon \
    --output-partial-info-plist "$STAGE/partial.plist" \
    --output-format human-readable-text >/dev/null

xcrun ibtool "$PACKAGING/LaunchScreen.storyboard" \
    --compile "$APP_BUNDLE/LaunchScreen.storyboardc" \
    --target-device iphone \
    --minimum-deployment-target "$DEPLOYMENT_TARGET" \
    --output-format human-readable-text >/dev/null

cp -f "$STAGE/Assets.car" "$APP_BUNDLE/Assets.car"
# Fallback files behind CFBundleIconFiles. actool emits an ~ipad variant regardless of
# --target-device; it is dropped, since UIDeviceFamily is iPhone only.
for icon in "$STAGE"/AppIcon*.png; do
    case "$icon" in
        *'~ipad.png') continue ;;
    esac
    cp -f "$icon" "$APP_BUNDLE/"
done

cp -f "$PACKAGING/PrivacyInfo.xcprivacy" "$APP_BUNDLE/PrivacyInfo.xcprivacy"

# App Store Connect rejects a re-upload that reuses a CFBundleVersion, so CI passes the run
# number. Locally any constant is fine; nothing local is ever uploaded.
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
BUILD_NUMBER="${VMUX_IOS_BUILD_NUMBER:-1}"

if [[ -z "$VERSION" ]]; then
    echo "Error: could not read version from $ROOT/Cargo.toml" >&2
    exit 1
fi

/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$PLIST"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $BUILD_NUMBER" "$PLIST"

echo "inject-ios-resources: $APP_BUNDLE (version $VERSION, build $BUILD_NUMBER)"
