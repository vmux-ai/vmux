#!/usr/bin/env bash
# Asserts the iOS .app bundle has the expected layout. Exits non-zero on failure.
#
# Everything checked here is injected after `dx build` rather than produced by it, so a silent
# regression in inject-ios-resources.sh would otherwise only surface as an App Store rejection.
#
# Pass --signed to additionally require the signing artifacts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/cargo-target-paths.sh"

SIGNED=0
APP=""
PROFILE="${VMUX_IOS_PROFILE:-debug}"
for arg in "$@"; do
    case "$arg" in
        --signed)
            SIGNED=1
            PROFILE="release"
            ;;
        *) APP="$arg" ;;
    esac
done

if [[ -z "$APP" ]]; then
    APP="$(vmux_cargo_target_dir "$ROOT")/dx/vmux_mobile/$PROFILE/ios/VmuxMobile.app"
fi

if [[ ! -d "$APP" ]]; then
    echo "usage: $0 [--signed] [path-to-VmuxMobile.app]" >&2
    echo "no bundle at $APP" >&2
    exit 1
fi

REQUIRED=(
    "vmux_mobile"
    "Info.plist"
    "Assets.car"
    "AppIcon60x60@2x.png"
    "LaunchScreen.storyboardc"
    "PrivacyInfo.xcprivacy"
)

if [[ "$SIGNED" == "1" ]]; then
    REQUIRED+=("embedded.mobileprovision" "_CodeSignature")
fi

for path in "${REQUIRED[@]}"; do
    if [[ ! -e "$APP/$path" ]]; then
        echo "MISSING: $APP/$path" >&2
        exit 1
    fi
done

# UIDeviceFamily is iPhone-only, so an iPad icon in the bundle means actool was run without
# --target-device iphone.
FORBIDDEN=(
    "AppIcon76x76@2x~ipad.png"
)

for path in "${FORBIDDEN[@]}"; do
    if [[ -e "$APP/$path" ]]; then
        echo "FORBIDDEN: $APP/$path" >&2
        exit 1
    fi
done

plist_value() {
    /usr/libexec/PlistBuddy -c "Print :$1" "$APP/Info.plist" 2>/dev/null || true
}

if [[ "$(plist_value CFBundleIdentifier)" != "ai.vmux.mobile" ]]; then
    echo "Info.plist CFBundleIdentifier is not ai.vmux.mobile" >&2
    exit 1
fi

if [[ "$(plist_value 'CFBundleIcons:CFBundlePrimaryIcon:CFBundleIconName')" != "AppIcon" ]]; then
    echo "Info.plist is missing CFBundleIconName" >&2
    exit 1
fi

if [[ "$(plist_value ITSAppUsesNonExemptEncryption)" != "false" ]]; then
    echo "Info.plist is missing ITSAppUsesNonExemptEncryption" >&2
    exit 1
fi

# The version keys are stamped onto the built copy, so a mismatch means the injection step did
# not run and the checked-in placeholders shipped instead.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
if [[ "$(plist_value CFBundleShortVersionString)" != "$VERSION" ]]; then
    echo "Info.plist CFBundleShortVersionString does not match Cargo.toml ($VERSION)" >&2
    exit 1
fi

echo "OK: iOS bundle layout correct"
