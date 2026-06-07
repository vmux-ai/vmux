#!/usr/bin/env bash
# Embed packaging/macos/ai.vmux.service.plist inside Vmux.app, substituting
# the build profile (and per-SHA label for local builds).
#
# Required env: VMUX_APP_BUNDLE, VMUX_BUILD_PROFILE
# Optional env: VMUX_GIT_HASH (required when VMUX_BUILD_PROFILE=local)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

: "${VMUX_APP_BUNDLE:?VMUX_APP_BUNDLE not set}"
: "${VMUX_BUILD_PROFILE:?VMUX_BUILD_PROFILE not set}"

PLIST_SRC="$ROOT/packaging/macos/ai.vmux.service.plist"
PLIST_DST="$VMUX_APP_BUNDLE/Contents/Library/LaunchAgents/ai.vmux.service.plist"
SERVICE_APP="$VMUX_APP_BUNDLE/Contents/Library/LoginItems/Vmux Service.app"
SERVICE_SRC="$VMUX_APP_BUNDLE/Contents/MacOS/Vmux Service"
SERVICE_EXEC="$SERVICE_APP/Contents/MacOS/Vmux Service"
SERVICE_INFO="$SERVICE_APP/Contents/Info.plist"
SERVICE_ICON="$SERVICE_APP/Contents/Resources/Vmux.icns"
PARENT_BUNDLE_ID="${VMUX_BUNDLE_ID:-ai.vmux.desktop}"

LABEL="ai.vmux.service"
if [[ "$VMUX_BUILD_PROFILE" == "local" ]]; then
    : "${VMUX_GIT_HASH:?VMUX_GIT_HASH must be set for local builds}"
    LABEL="ai.vmux.service.$VMUX_GIT_HASH"
fi

mkdir -p "$SERVICE_APP/Contents/MacOS" "$SERVICE_APP/Contents/Resources"
if [[ -f "$SERVICE_SRC" ]]; then
    mv -f "$SERVICE_SRC" "$SERVICE_EXEC"
elif [[ ! -f "$SERVICE_EXEC" ]]; then
    echo "Missing service executable: $SERVICE_SRC" >&2
    exit 1
fi

cp -f "$ROOT/packaging/macos/Vmux.icns" "$SERVICE_ICON"
cat > "$SERVICE_INFO" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>Vmux Service</string>
  <key>CFBundleExecutable</key>
  <string>Vmux Service</string>
  <key>CFBundleIdentifier</key>
  <string>$LABEL</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Vmux Service</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleIconFile</key>
  <string>Vmux</string>
  <key>LSUIElement</key>
  <true/>
</dict>
</plist>
EOF

mkdir -p "$(dirname "$PLIST_DST")"
sed -e "s|{{PROFILE}}|$VMUX_BUILD_PROFILE|g" \
    -e "s|<string>ai.vmux.service</string>|<string>$LABEL</string>|" \
    -e "s|<string>ai.vmux.desktop</string>|<string>$PARENT_BUNDLE_ID</string>|" \
    "$PLIST_SRC" > "$PLIST_DST"
echo "==> Embedded launchd plist (label=$LABEL, profile=$VMUX_BUILD_PROFILE)"
