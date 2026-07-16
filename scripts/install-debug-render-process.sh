#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO_BIN:-cargo}"
rustc_bin="${RUSTC:-rustc}"
cef_framework_dir="${CEF_FRAMEWORK_DIR:-$HOME/.local/share/Chromium Embedded Framework.framework}"
installed_dir="$cef_framework_dir/Libraries"
helper_name="bevy_cef_debug_render_process"
installed_helper="$installed_dir/$helper_name"
stamp="$installed_dir/.vmux-$helper_name.fingerprint"
target_setting="${CARGO_TARGET_DIR:-target}"
tmp_install=""
tmp_stamp=""
tmp_cache=""

source "$root/scripts/target-cache-common.sh"

case "$(uname -s)" in
    Darwin) helper_cache="${VMUX_CEF_HELPER_CACHE:-$HOME/Library/Caches/Vmux/cef-debug-render-process}" ;;
    Linux) helper_cache="${VMUX_CEF_HELPER_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/vmux/cef-debug-render-process}" ;;
    *) echo "CEF debug render process install unsupported on $(uname -s)" >&2; exit 1 ;;
esac

if [[ ! -d "$installed_dir" ]]; then
    echo "CEF framework libraries not found: $installed_dir" >&2
    exit 1
fi

cleanup() {
    vmux_target_lock_release 9
    [[ -z "$tmp_install" ]] || rm -f -- "$tmp_install"
    [[ -z "$tmp_stamp" ]] || rm -f -- "$tmp_stamp"
    [[ -z "$tmp_cache" ]] || rm -f -- "$tmp_cache"
}
trap cleanup EXIT

mkdir -p "$helper_cache"
vmux_target_lock_acquire "$helper_cache" "$root" 9

input_paths=(
    "$root/Cargo.lock"
    "$root/Cargo.toml"
    "$root/scripts/cargo-with-cef-cache.sh"
    "$root/scripts/install-debug-render-process.sh"
)
if [[ -f "$root/rust-toolchain.toml" ]]; then
    input_paths+=("$root/rust-toolchain.toml")
fi
for directory in \
    "$root/patches/bevy_cef_core-0.5.2" \
    "$root/patches/bevy_cef_debug_render_process-0.5.2" \
    "$root/patches/bevy_remote-0.19.0" \
    "$root/patches/bevy_window-0.19.0"
do
    while IFS= read -r path; do
        input_paths+=("$path")
    done < <(
        find "$directory" -path "$directory/target" -prune -o -type f -print \
            | LC_ALL=C sort
    )
done

fingerprint="$({
    printf 'cargo=%s\n' "$("$cargo_bin" -V)"
    printf 'rustc=%s\n' "$("$rustc_bin" -vV)"
    printf 'rustc-wrapper=%s\n' "${RUSTC_WRAPPER:-}"
    printf 'rustflags=%s\n' "${RUSTFLAGS:-}"
    printf 'cargo-encoded-rustflags=%s\n' "${CARGO_ENCODED_RUSTFLAGS:-}"
    printf 'cargo-build-target=%s\n' "${CARGO_BUILD_TARGET:-}"
    printf 'macosx-deployment-target=%s\n' "${MACOSX_DEPLOYMENT_TARGET:-}"
    printf 'vmux-cef-sdk-cache=%s\n' "${VMUX_CEF_SDK_CACHE:-}"
    env | LC_ALL=C sort | sed -n '/^CARGO_PROFILE_DEV_/p'
    for path in "${input_paths[@]}"; do
        printf 'file=%s\n' "${path#"$root/"}"
    done
    git -C "$root" hash-object "${input_paths[@]}"
} | git -C "$root" hash-object --stdin)"

if [[ -x "$installed_helper" && -f "$stamp" && "$(<"$stamp")" == "$fingerprint" ]]; then
    echo "CEF debug render process up to date"
    exit 0
fi

cache_entry="$helper_cache/$fingerprint"
cached_helper="$cache_entry/$helper_name"

install_helper() {
    local source="$1"
    tmp_install="$(mktemp "$installed_dir/.vmux-$helper_name.XXXXXX")"
    cp "$source" "$tmp_install"
    chmod 755 "$tmp_install"
    mv -f "$tmp_install" "$installed_helper"
    tmp_install=""
    tmp_stamp="$(mktemp "$installed_dir/.vmux-$helper_name-stamp.XXXXXX")"
    printf '%s\n' "$fingerprint" > "$tmp_stamp"
    mv -f "$tmp_stamp" "$stamp"
    tmp_stamp=""
}

if [[ -x "$cached_helper" ]]; then
    install_helper "$cached_helper"
    echo "Installed cached CEF debug render process"
    exit 0
fi

cd "$root"
CARGO_BIN="$cargo_bin" "$root/scripts/cargo-with-cef-cache.sh" \
    build -p bevy_cef_debug_render_process --features debug

if [[ "$target_setting" == /* ]]; then
    target_dir="$target_setting"
else
    target_dir="$root/$target_setting"
fi
if [[ -n "${CARGO_BUILD_TARGET:-}" ]]; then
    target_dir="$target_dir/$CARGO_BUILD_TARGET"
fi
built_helper="$target_dir/debug/$helper_name"
if [[ ! -x "$built_helper" ]]; then
    echo "CEF debug render process build output not found: $built_helper" >&2
    exit 1
fi

mkdir -p "$cache_entry"
tmp_cache="$(mktemp "$cache_entry/.$helper_name.XXXXXX")"
cp "$built_helper" "$tmp_cache"
chmod 755 "$tmp_cache"
mv -f "$tmp_cache" "$cached_helper"
tmp_cache=""
install_helper "$cached_helper"
echo "Built and cached CEF debug render process"
