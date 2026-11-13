#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/cargo-target-paths.sh"
source "$ROOT/scripts/product-host.sh"
vmux_host_read "$ROOT"
release_dir="${VMUX_CARGO_RELEASE_DIR:-$(vmux_cargo_profile_dir "$ROOT" release)}"
app_bundle="${VMUX_APP_BUNDLE:-$release_dir/$VMUX_HOST_NAME.app}"

"$ROOT/scripts/inject-cef.sh"

mkdir -p "$app_bundle/Contents/Resources"
cp "$VMUX_HOST_TRAY_ICON" "$app_bundle/Contents/Resources/tray.png"

if [[ "${CARGO_PACKAGER_FORMAT:-}" == "dmg" ]]; then
    VMUX_APP_BUNDLE="$app_bundle" "$ROOT/scripts/embed-launch-agent-plist.sh"
    APP_BUNDLE="$app_bundle" "$ROOT/scripts/sign-and-notarize.sh"
fi
