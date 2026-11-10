#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root/scripts/cargo-target-paths.sh"
debug_dir="$(vmux_cargo_profile_dir "$root" debug)"
app="$debug_dir/Vmux Dev.app"
contents="$app/Contents"
identity="${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY not set}"
entitlements="$root/packaging/macos/VmuxDev.entitlements"
plist_buddy="${PLIST_BUDDY:-/usr/libexec/PlistBuddy}"
mkdir -p "$contents/MacOS" "$contents/Resources" "$contents/Frameworks"
temporary="$(mktemp -d "$debug_dir/.vmux-dev-bundle.XXXXXX")"
trap 'rm -r "$temporary"' EXIT

copy_changed() {
    local source="$1"
    local destination="$2"
    if [[ ! -f "$destination" ]] || ! cmp -s "$source" "$destination"; then
        cp "$source" "$destination"
    fi
}

fingerprint() {
    printf '%s\n' "$identity"
    shasum -a 256 "$entitlements" "$@"
}

bundle_current() {
    local inputs="$1"
    local stamp="$2"
    local binary="$3"
    [[ -f "$stamp" && -f "$binary" ]] || return 1
    { cat "$inputs"; shasum -a 256 "$binary"; } > "$temporary/current"
    cmp -s "$temporary/current" "$stamp"
}

record_bundle() {
    local inputs="$1"
    local stamp="$2"
    local binary="$3"
    { cat "$inputs"; shasum -a 256 "$binary"; } > "$stamp.tmp"
    mv -f "$stamp.tmp" "$stamp"
}

plist="$temporary/Info.plist"
cp "$root/packaging/macos/Info.plist" "$plist"
"$plist_buddy" -c 'Set :CFBundleIdentifier ai.vmux.desktop.dev' "$plist"
"$root/scripts/build-title.sh" dev "$root" "$plist" >/dev/null
"$plist_buddy" -c "Merge '$root/packaging/macos/usage-descriptions.plist'" "$plist"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)"
"$plist_buddy" -c "Set :CFBundleShortVersionString $version" "$plist"
"$plist_buddy" -c "Set :CFBundleVersion $version" "$plist"
copy_changed "$plist" "$contents/Info.plist"
copy_changed "$root/packaging/macos/Vmux.icns" "$contents/Resources/Vmux.icns"
copy_changed "$debug_dir/vmux" "$contents/MacOS/vmux"

service_app="$contents/Library/LoginItems/Vmux Service.app"
service="$service_app/Contents"
mkdir -p "$service/MacOS"
cp "$plist" "$temporary/Service.plist"
"$plist_buddy" -c 'Set :CFBundleExecutable Vmux Service' "$temporary/Service.plist"
"$plist_buddy" -c 'Set :CFBundleIdentifier ai.vmux.service.dev' "$temporary/Service.plist"
"$plist_buddy" -c 'Set :CFBundleName Vmux Service' "$temporary/Service.plist"
"$plist_buddy" -c 'Set :CFBundleDisplayName Vmux Service' "$temporary/Service.plist"
"$plist_buddy" -c 'Set :LSUIElement true' "$temporary/Service.plist"
copy_changed "$temporary/Service.plist" "$service/Info.plist"
fingerprint "$debug_dir/vmux_service" "$service/Info.plist" > "$temporary/service-inputs"
service_stamp="$debug_dir/.vmux-dev-service-signed"
service_binary="$service/MacOS/Vmux Service"
if ! bundle_current "$temporary/service-inputs" "$service_stamp" "$service_binary" \
    || ! codesign --verify --strict "$service_app" >/dev/null 2>&1; then
    cp "$debug_dir/vmux_service" "$service_binary"
    codesign --force --sign "$identity" --options runtime --entitlements "$entitlements" "$service_app"
    record_bundle "$temporary/service-inputs" "$service_stamp" "$service_binary"
fi

link_binary="$contents/MacOS/vmux_desktop"
stdlib="$(rustc --print target-libdir)"
{
    fingerprint "$debug_dir/vmux_desktop" "$contents/MacOS/vmux" "$contents/Info.plist" \
        "$contents/Resources/Vmux.icns" "$service_stamp"
    printf '%s\n' "$debug_dir/deps" "$stdlib"
} > "$temporary/desktop-inputs"
desktop_stamp="$debug_dir/.vmux-dev-desktop-signed"
if ! bundle_current "$temporary/desktop-inputs" "$desktop_stamp" "$link_binary" \
    || ! codesign --verify --strict "$app" >/dev/null 2>&1; then
    cp "$debug_dir/vmux_desktop" "$link_binary"
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
    codesign --force --sign "$identity" --options runtime --entitlements "$entitlements" "$app"
    record_bundle "$temporary/desktop-inputs" "$desktop_stamp" "$link_binary"
fi
codesign --verify --strict "$app"
printf 'Debug app ready: %s\n' "$app"
