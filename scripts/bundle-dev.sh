#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root/scripts/cargo-target-paths.sh"
source "$root/scripts/product-host.sh"
vmux_host_read "$root"
debug_dir="$(vmux_cargo_profile_dir "$root" debug)"
app="$debug_dir/$VMUX_HOST_NAME Dev.app"
contents="$app/Contents"
identity="${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY not set}"
entitlements="$VMUX_HOST_DEV_ENTITLEMENTS"
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
python3 - "$root/packaging/macos/Info.plist" "$root/Cargo.toml" "$plist" "$temporary/Service.plist" <<'PY'
import json, os, pathlib, plistlib, re, sys
template, cargo, desktop, service = map(pathlib.Path, sys.argv[1:5])
metadata = json.loads(os.environ['VMUX_HOST_JSON'])
with template.open('rb') as source:
    plist = plistlib.load(source)
with open(metadata['VMUX_HOST_USAGE_DESCRIPTIONS'], 'rb') as source:
    plist.update(plistlib.load(source))
version = re.search(r'^version = "([^"\n]+)"$', cargo.read_text(), re.M).group(1)
name = metadata['VMUX_HOST_NAME']+' Dev'
plist.update(CFBundleIdentifier=metadata['VMUX_HOST_BUNDLE_ID']+'.dev', CFBundleDisplayName=name,
    CFBundleName=name, CFBundleShortVersionString=version, CFBundleVersion=version)
with desktop.open('wb') as output:
    plistlib.dump(plist, output)
name = metadata['VMUX_HOST_SERVICE_NAME']
plist.update(CFBundleExecutable=name, CFBundleIdentifier=metadata['VMUX_HOST_SERVICE_ID']+'.dev',
    CFBundleName=name, CFBundleDisplayName=name, LSUIElement=True)
with service.open('wb') as output:
    plistlib.dump(plist, output)
PY
"$root/scripts/build-title.sh" dev "$root" "$plist" >/dev/null
copy_changed "$plist" "$contents/Info.plist"
copy_changed "$VMUX_HOST_ICON" "$contents/Resources/Vmux.icns"
copy_changed "$VMUX_HOST_TRAY_ICON" "$contents/Resources/tray.png"
copy_changed "$debug_dir/vmux" "$contents/MacOS/vmux"

service_app="$contents/Library/LoginItems/$VMUX_HOST_SERVICE_BUNDLE"
service="$service_app/Contents"
mkdir -p "$service/MacOS"
copy_changed "$temporary/Service.plist" "$service/Info.plist"
fingerprint "$debug_dir/vmux_service" "$service/Info.plist" > "$temporary/service-inputs"
service_stamp="$debug_dir/.vmux-dev-service-signed"
service_binary="$service/MacOS/$VMUX_HOST_SERVICE_NAME"
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
        "$contents/Resources/Vmux.icns" "$contents/Resources/tray.png" "$service_stamp"
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
