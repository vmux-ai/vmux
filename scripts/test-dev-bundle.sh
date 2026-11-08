#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root/scripts/cargo-target-paths.sh"
app="$(vmux_cargo_profile_dir "$root" debug)/Vmux Dev.app"
plist="$app/Contents/Info.plist"
[[ -f "$plist" ]]
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")" == vmux_desktop ]]
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")" == ai.vmux.desktop.dev ]]
[[ "$(/usr/libexec/PlistBuddy -c 'Print :NSCameraUsageDescription' "$plist")" == *camera* ]]
[[ "$(/usr/libexec/PlistBuddy -c 'Print :NSMicrophoneUsageDescription' "$plist")" == *microphone* ]]
[[ -x "$app/Contents/MacOS/vmux" ]]
[[ -x "$app/Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service" ]]
[[ -f "$app/Contents/Resources/Vmux.icns" ]]
[[ ! -d "$app/Contents/Frameworks/Chromium Embedded Framework.framework" ]]
debug_dir="$(vmux_cargo_profile_dir "$root" debug)"
stdlib="$(rustc --print target-libdir)"
otool -l "$app/Contents/MacOS/vmux_desktop" | grep -Fq "path $debug_dir/deps ("
otool -l "$app/Contents/MacOS/vmux_desktop" | grep -Fq "path $stdlib ("
otool -L "$app/Contents/MacOS/vmux_desktop" | grep -q '@rpath/libbevy_dylib'
codesign --verify --strict "$app"
printf 'Debug app bundle layout passed\n'
