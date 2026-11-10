#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "${1:-}" != --fixture ]]; then
    source "$root/scripts/cargo-target-paths.sh"
    app="$(vmux_cargo_profile_dir "$root" debug)/Vmux Dev.app"
    plist="$app/Contents/Info.plist"
    [[ -f "$plist" ]]
    launch_alias="$(vmux_cargo_profile_dir "$root" release)/Vmux Dev.app"
    [[ -L "$launch_alias" && -f "$launch_alias/Contents/Info.plist" ]]
    [[ "$(readlink "$launch_alias")" == "$app" ]]
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")" == vmux_desktop ]]
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")" == ai.vmux.desktop.dev ]]
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :LSUIElement' "$plist")" == false ]]
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :LSUIElement' "$app/Contents/Library/LoginItems/Vmux Service.app/Contents/Info.plist")" == true ]]
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
    exit 0
fi

tmp="$(mktemp -d)"
tmp="$(cd "$tmp" && pwd -P)"
trap 'rm -r "$tmp"' EXIT
fixture="$tmp/checkout with spaces"
target="$fixture/output with spaces"
mkdir -p "$fixture/scripts" "$fixture/packaging/macos" "$target/debug/deps" "$tmp/bin"
cp "$root/scripts/bundle-dev.sh" "$root/scripts/cargo-target-paths.sh" "$fixture/scripts/"
cp "$root/packaging/macos/Info.plist" "$root/packaging/macos/usage-descriptions.plist" \
    "$root/packaging/macos/VmuxDev.entitlements" "$root/packaging/macos/Vmux.icns" "$fixture/packaging/macos/"
printf '[workspace.package]\nversion = "0.1.2"\n' > "$fixture/Cargo.toml"
for binary in vmux vmux_service vmux_desktop; do
    printf 'compiled %s\n' "$binary" > "$target/debug/$binary"
done
cat > "$tmp/bin/codesign" <<'SIGN'
#!/usr/bin/env bash
set -euo pipefail
if [[ " $* " == *' --force '* ]]; then
    printf '%s\n' "$*" >> "$TEST_SIGN_LOG"
    bundle="${!#}"
    if [[ "$bundle" == *'/Vmux Service.app' ]]; then
        binary="$bundle/Contents/MacOS/Vmux Service"
    else
        binary="$bundle/Contents/MacOS/vmux_desktop"
    fi
    printf 'signed\n' >> "$binary"
fi
SIGN
cat > "$tmp/bin/rustc" <<'RUSTC'
#!/usr/bin/env bash
printf '/toolchain/lib\n'
RUSTC
cat > "$tmp/bin/otool" <<'OTOOL'
#!/usr/bin/env bash
if [[ "$1" == -l ]]; then
    printf '          path %s (offset 12)\n' "$CARGO_TARGET_DIR/debug/deps" /toolchain/lib
fi
OTOOL
cat > "$tmp/bin/install_name_tool" <<'INSTALL'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$TEST_LINK_LOG"
INSTALL
chmod +x "$tmp/bin/"* "$fixture/scripts/"*.sh
export PATH="$tmp/bin:$PATH"
export CARGO_TARGET_DIR="$target"
export APPLE_SIGNING_IDENTITY='fixture identity'
export TEST_SIGN_LOG="$tmp/sign.log"
export TEST_LINK_LOG="$tmp/link.log"
if [[ -x "${PLIST_BUDDY:-/usr/libexec/PlistBuddy}" ]]; then
    plist_buddy="${PLIST_BUDDY:-/usr/libexec/PlistBuddy}"
else
    cat > "$tmp/bin/PlistBuddy" <<'PLIST'
#!/usr/bin/env python3
import plistlib
import shlex
import sys
command, filename = sys.argv[2], sys.argv[3]
with open(filename, 'rb') as source:
    data = plistlib.load(source)
parts = shlex.split(command)
if parts[0] == 'Merge':
    with open(parts[1], 'rb') as source:
        data.update(plistlib.load(source))
elif parts[0] == 'Set':
    key = parts[1].removeprefix(':')
    value = ' '.join(parts[2:])
    if isinstance(data.get(key), bool):
        assert value in ('true', 'false')
        value = value == 'true'
    data[key] = value
else:
    raise SystemExit(f'Unsupported plist operation: {command}')
with open(filename, 'wb') as destination:
    plistlib.dump(data, destination)
PLIST
    chmod +x "$tmp/bin/PlistBuddy"
    plist_buddy="$tmp/bin/PlistBuddy"
fi
export PLIST_BUDDY="$plist_buddy"
app="$target/debug/Vmux Dev.app"
service="$app/Contents/Library/LoginItems/Vmux Service.app"
"$fixture/scripts/bundle-dev.sh"
[[ "$(wc -l < "$TEST_SIGN_LOG" | tr -d ' ')" == 2 ]]
service_before="$(shasum -a 256 "$service/Contents/MacOS/Vmux Service")"
mtime_before="$(python3 -c 'import os,sys; print(os.stat(sys.argv[1]).st_mtime_ns)' "$service/Contents/MacOS/Vmux Service")"
"$fixture/scripts/bundle-dev.sh"
[[ "$(wc -l < "$TEST_SIGN_LOG" | tr -d ' ')" == 2 ]]
[[ "$service_before" == "$(shasum -a 256 "$service/Contents/MacOS/Vmux Service")" ]]
[[ "$mtime_before" == "$(python3 -c 'import os,sys; print(os.stat(sys.argv[1]).st_mtime_ns)' "$service/Contents/MacOS/Vmux Service")" ]]
printf 'changed desktop\n' >> "$target/debug/vmux_desktop"
"$fixture/scripts/bundle-dev.sh"
[[ "$(wc -l < "$TEST_SIGN_LOG" | tr -d ' ')" == 3 ]]
[[ "$service_before" == "$(shasum -a 256 "$service/Contents/MacOS/Vmux Service")" ]]
[[ "$mtime_before" == "$(python3 -c 'import os,sys; print(os.stat(sys.argv[1]).st_mtime_ns)' "$service/Contents/MacOS/Vmux Service")" ]]
"$plist_buddy" -c 'Set :NSMicrophoneUsageDescription Updated microphone prompt' "$fixture/packaging/macos/usage-descriptions.plist"
"$plist_buddy" -c 'Set :LSMinimumSystemVersion 14.0' "$fixture/packaging/macos/Info.plist"
"$fixture/scripts/bundle-dev.sh"
[[ "$(wc -l < "$TEST_SIGN_LOG" | tr -d ' ')" == 5 ]]
python3 - "$app/Contents/Info.plist" "$service/Contents/Info.plist" <<'VERIFY'
import plistlib
import sys
for index, filename in enumerate(sys.argv[1:]):
    with open(filename, 'rb') as source:
        data = plistlib.load(source)
    assert data['NSMicrophoneUsageDescription'] == 'Updated microphone prompt'
    assert data['NSCameraUsageDescription'].startswith('Vmux needs camera access')
    assert data['LSMinimumSystemVersion'] == '14.0'
    assert data['CFBundleShortVersionString'] == '0.1.2'
    assert data['LSUIElement'] is (index == 1)
VERIFY
printf 'Debug bundle templates, spaced paths, and no-op service identity passed\n'
