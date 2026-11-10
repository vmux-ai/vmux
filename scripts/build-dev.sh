#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
source "$root/scripts/cargo-target-paths.sh"
if ! "$root/scripts/vmux-builder" inherited; then
    exec "$root/scripts/vmux-builder" dev "$0" "$@"
fi
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"
export VMUX_BUILD_PROFILE=dev

debug_dir="$(vmux_cargo_profile_dir "$root" debug)"
stamp="$debug_dir/.vmux-dev-ready"
mkdir -p "$debug_dir"
rm -f "$stamp"
release_dir="$(vmux_cargo_profile_dir "$root" release)"
alias="$release_dir/Vmux Dev.app"
if [[ -L "$alias" ]]; then
    rm -f "$alias"
fi

cd "$root"
"$root/scripts/cargo-with-cef-cache.sh" build -p vmux_cli
"$root/scripts/cargo-with-cef-cache.sh" build -p vmux_app --bin vmux_service --features service-daemon
if [[ "$(uname -s)" == Darwin ]]; then
    "$root/scripts/cargo-with-cef-cache.sh" rustc -p vmux_desktop --bin vmux_desktop "$@" -- -C link-arg=-Wl,-headerpad,1000
else
    "$root/scripts/cargo-with-cef-cache.sh" build -p vmux_desktop "$@"
fi

if [[ "$(uname -s)" == Darwin ]]; then
    identity="$("$root/scripts/ensure-local-codesign-identity.sh")"
    APPLE_SIGNING_IDENTITY="$identity" APP_BINARY="$debug_dir/vmux_desktop" \
        HELPER_BINARY="${CEF_DEBUG_RENDER:-$HOME/.local/share/Chromium Embedded Framework.framework/Libraries/bevy_cef_debug_render_process}" \
        "$root/scripts/sign-dev-mac.sh"
    APPLE_SIGNING_IDENTITY="$identity" "$root/scripts/bundle-dev.sh"
fi

{
    printf '%s\n' "$root"
    for binary in vmux vmux_service vmux_desktop; do
        [[ -x "$debug_dir/$binary" ]] || { echo "Missing debug binary: $binary" >&2; exit 1; }
        shasum -a 256 "$debug_dir/$binary"
    done
    if [[ "$(uname -s)" == Darwin ]]; then
        shasum -a 256 "$debug_dir/Vmux Dev.app/Contents/MacOS/vmux_desktop"
    fi
} > "$stamp.tmp"
if [[ "$(uname -s)" == Darwin ]]; then
    mkdir -p "$release_dir"
    if [[ -e "$alias" && ! -L "$alias" ]]; then
        echo "Cannot publish debug app over an existing bundle: $alias" >&2
        exit 1
    fi
    ln -sfn "$debug_dir/Vmux Dev.app" "$alias"
fi
mv -f "$stamp.tmp" "$stamp"
printf 'Debug build ready: %s\nRun: make run-dev\n' "$debug_dir/vmux_desktop"
