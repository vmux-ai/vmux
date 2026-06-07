#!/usr/bin/env bash
# Asserts the .app bundle has the expected layout. Exits non-zero on failure.
set -euo pipefail
APP="${1:?usage: $0 <path-to-Vmux.app>}"

REQUIRED=(
    "Contents/MacOS/Vmux"
    "Contents/MacOS/vmux"
    "Contents/Frameworks/Vmux Helper.app/Contents/MacOS/Vmux Helper"
    "Contents/Frameworks/Vmux Helper.app/Contents/Resources/Vmux.icns"
    "Contents/Frameworks/Vmux Helper (Renderer).app/Contents/MacOS/Vmux Helper (Renderer)"
    "Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service"
    "Contents/Library/LoginItems/Vmux Service.app/Contents/Resources/Vmux.icns"
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

FORBIDDEN=(
    "Contents/MacOS/vmux_desktop"
    "Contents/MacOS/vmux_service"
    "Contents/Frameworks/vmux_desktop Helper.app"
)

for path in "${FORBIDDEN[@]}"; do
    if [[ -e "$APP/$path" ]]; then
        echo "FORBIDDEN: $APP/$path" >&2
        exit 1
    fi
done

echo "OK: bundle layout correct"
