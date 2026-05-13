#!/usr/bin/env bash
# Asserts the .app bundle has the expected layout. Exits non-zero on failure.
set -euo pipefail
APP="${1:?usage: $0 <path-to-Vmux.app>}"

REQUIRED=(
    "Contents/MacOS/vmux_desktop"
    "Contents/MacOS/vmux"
    "Contents/MacOS/vmux_service"
    "Contents/Library/LaunchAgents/ai.vmux.service.plist"
    "Contents/Info.plist"
    "Contents/Resources/Vmux.icns"
)

for path in "${REQUIRED[@]}"; do
    if [[ ! -e "$APP/$path" ]]; then
        echo "MISSING: $APP/$path" >&2
        exit 1
    fi
done

if grep -q '{{PROFILE}}' "$APP/Contents/Library/LaunchAgents/ai.vmux.service.plist"; then
    echo "Plist still has {{PROFILE}} placeholder — substitution did not run" >&2
    exit 1
fi

echo "OK: bundle layout correct"
