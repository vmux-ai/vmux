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

LABEL="ai.vmux.service"
if [[ "$VMUX_BUILD_PROFILE" == "local" ]]; then
    : "${VMUX_GIT_HASH:?VMUX_GIT_HASH must be set for local builds}"
    LABEL="ai.vmux.service.$VMUX_GIT_HASH"
fi

mkdir -p "$(dirname "$PLIST_DST")"
sed -e "s|{{PROFILE}}|$VMUX_BUILD_PROFILE|g" \
    -e "s|<string>ai.vmux.service</string>|<string>$LABEL</string>|" \
    "$PLIST_SRC" > "$PLIST_DST"
echo "==> Embedded launchd plist (label=$LABEL, profile=$VMUX_BUILD_PROFILE)"
