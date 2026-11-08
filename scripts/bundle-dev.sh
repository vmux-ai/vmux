#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root/scripts/cargo-target-paths.sh"
debug_dir="$(vmux_cargo_profile_dir "$root" debug)"
app="$debug_dir/Vmux Dev.app"
contents="$app/Contents"
identity="${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY not set}"
mkdir -p "$contents/MacOS" "$contents/Resources" "$contents/Frameworks"

copy_changed() {
    local source="$1"
    local destination="$2"
    if [[ ! -f "$destination" ]] || ! cmp -s "$source" "$destination"; then
        cp "$source" "$destination"
    fi
}

copy_changed "$debug_dir/vmux_desktop" "$contents/MacOS/vmux_desktop"
copy_changed "$debug_dir/vmux" "$contents/MacOS/vmux"
service="$contents/Library/LoginItems/Vmux Service.app/Contents"
mkdir -p "$service/MacOS"
copy_changed "$debug_dir/vmux_service" "$service/MacOS/Vmux Service"
copy_changed "$root/packaging/macos/Vmux.icns" "$contents/Resources/Vmux.icns"

plist="$contents/Info.plist"
if [[ ! -f "$plist" ]]; then
    cp "$root/packaging/macos/Info.plist" "$plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier ai.vmux.desktop.dev' "$plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleDisplayName Vmux Dev' "$plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleName Vmux Dev' "$plist"
    /usr/libexec/PlistBuddy -c "Merge $root/packaging/macos/usage-descriptions.plist" "$plist"
fi
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$plist"

if [[ ! -f "$service/Info.plist" ]]; then
    cp "$plist" "$service/Info.plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleExecutable Vmux Service' "$service/Info.plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier ai.vmux.service.dev' "$service/Info.plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleName Vmux Service' "$service/Info.plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleDisplayName Vmux Service' "$service/Info.plist"
fi

link_binary="$contents/MacOS/vmux_desktop"
stdlib="$(rustc --print target-libdir)"
for library_dir in "$debug_dir/deps" "$stdlib"; do
    if ! otool -l "$link_binary" | grep -Fq "path $library_dir ("; then
        install_name_tool -add_rpath "$library_dir" "$link_binary"
    fi
done
while IFS= read -r library; do
    name="$(basename "$library")"
    [[ -f "$debug_dir/deps/$name" ]] || { echo "Missing Bevy library: $name" >&2; exit 1; }
    if [[ "$library" != "@rpath/$name" ]]; then
        install_name_tool -change "$library" "@rpath/$name" "$link_binary"
    fi
done < <(otool -L "$link_binary" | awk '/libbevy_dylib/ {sub(/^[[:space:]]+/, ""); sub(/ \(compatibility.*/, ""); print}')

for bundle in "$contents/Library/LoginItems/Vmux Service.app" "$app"; do
    codesign --force --sign "$identity" --options runtime \
        --entitlements "$root/packaging/macos/VmuxDev.entitlements" "$bundle"
done
codesign --verify --strict "$app"
printf 'Debug app ready: %s\n' "$app"
