#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO_BIN:-cargo}"
cef_cache="${VMUX_CEF_SDK_CACHE:-}"
target_seed_key=""
cargo_pid=""
cargo_status=0

source "$root/scripts/target-cache-common.sh"

if [[ -z "$cef_cache" && "${CI:-}" != "true" ]]; then
    case "$(uname -s)" in
        Darwin) cef_cache="$HOME/Library/Caches/Vmux/cef-sdk" ;;
        *) cef_cache="${XDG_CACHE_HOME:-$HOME/.cache}/vmux/cef-sdk" ;;
    esac
fi

if sccache_bin="$(command -v sccache 2>/dev/null)"; then
    export CMAKE_C_COMPILER_LAUNCHER="${CMAKE_C_COMPILER_LAUNCHER:-$sccache_bin}"
    export CMAKE_CXX_COMPILER_LAUNCHER="${CMAKE_CXX_COMPILER_LAUNCHER:-$sccache_bin}"
    if [[ "${CI:-}" != "true" ]]; then
        export RUSTC_WRAPPER="${RUSTC_WRAPPER:-$sccache_bin}"
        if [[ "$RUSTC_WRAPPER" == "$sccache_bin" ]]; then
            export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
        fi
    fi
fi

cleanup() {
    vmux_target_lock_release 8
}

forward_signal() {
    local signal="$1"
    if [[ -n "$cargo_pid" ]] && kill -0 "$cargo_pid" 2>/dev/null; then
        kill -s "$signal" "$cargo_pid" 2>/dev/null || true
    fi
}

if [[ "${CI:-}" != "true" ]]; then
    target_seed_key="$(CARGO_BIN="$cargo_bin" "$root/scripts/target-seed-key.sh")"
    export VMUX_TARGET_SEED_KEY="$target_seed_key"
    if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
        case "$CARGO_TARGET_DIR" in
            /*) target_dir="$CARGO_TARGET_DIR" ;;
            *) target_dir="$PWD/$CARGO_TARGET_DIR" ;;
        esac
    else
        target_dir="$root/target"
    fi
    target_parent="$(dirname "$target_dir")"
    mkdir -p "$target_parent"
    target_parent="$(cd "$target_parent" && pwd)"
    target_dir="$target_parent/$(basename "$target_dir")"
    export VMUX_TARGET_DESTINATION="$target_dir"
    "$root/scripts/seed-worktree-target.sh" --if-needed "$root"
    vmux_target_lock_acquire "$target_dir" "$root" 8
    trap cleanup EXIT
fi

trap 'forward_signal HUP' HUP
trap 'forward_signal INT' INT
trap 'forward_signal TERM' TERM

set +e
if [[ -n "$cef_cache" ]]; then
    mkdir -p "$cef_cache"
    env CEF_PATH="$cef_cache" "$cargo_bin" "$@" &
else
    env -u CEF_PATH "$cargo_bin" "$@" &
fi
cargo_pid=$!
while true; do
    wait "$cargo_pid"
    cargo_status=$?
    if [[ "$cargo_status" -gt 128 ]] && kill -0 "$cargo_pid" 2>/dev/null; then
        continue
    fi
    break
done
cargo_pid=""
set -e

if [[ "$cargo_status" != "0" ]]; then
    exit "$cargo_status"
fi

case "${1:-}" in
    bench | build | check | clippy | doc | rustc | test)
        artifact_command=1
        ;;
    *)
        artifact_command=0
        ;;
esac

if [[ -n "$target_seed_key" && "$artifact_command" == "1" ]]; then
    rm -rf -- "$target_dir/.vmux-seed"
    mkdir -p "$target_dir/.vmux-seed"
    touch "$target_dir/.vmux-seed/$target_seed_key"
fi
