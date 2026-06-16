#!/usr/bin/env bash
# Asserts the .app bundle has the expected layout. Exits non-zero on failure.
set -euo pipefail
APP="${1:?usage: $0 <path-to-Vmux.app>}"

REQUIRED=(
    "Contents/MacOS/vmux_desktop"
    "Contents/MacOS/vmux"
    "Contents/Frameworks/vmux_desktop Helper.app/Contents/MacOS/vmux_desktop Helper"
    "Contents/Frameworks/vmux_desktop Helper.app/Contents/Resources/Vmux.icns"
    "Contents/Frameworks/vmux_desktop Helper (Renderer).app/Contents/MacOS/vmux_desktop Helper (Renderer)"
    "Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service"
    "Contents/Library/LoginItems/Vmux Service.app/Contents/Resources/Vmux.icns"
    "Contents/Library/LaunchAgents/ai.vmux.service.plist"
    "Contents/Info.plist"
    "Contents/Resources/Vmux.icns"
    "Contents/Resources/crash_reporter.cfg"
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
    "Contents/MacOS/vmux_service"
    "Contents/Frameworks/Vmux Helper.app"
)

for path in "${FORBIDDEN[@]}"; do
    if [[ -e "$APP/$path" ]]; then
        echo "FORBIDDEN: $APP/$path" >&2
        exit 1
    fi
done

if cmp -s "$APP/Contents/MacOS/vmux" "$APP/Contents/MacOS/vmux_desktop"; then
    echo "COLLISION: Contents/MacOS/vmux is identical to vmux_desktop (case-insensitive name clash)" >&2
    exit 1
fi

echo "OK: bundle layout correct"
