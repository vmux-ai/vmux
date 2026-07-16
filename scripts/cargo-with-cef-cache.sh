#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO_BIN:-cargo}"
cef_cache="${VMUX_CEF_SDK_CACHE:-}"
linked_worktree=0

if git_dir="$(git -C "$root" rev-parse --path-format=absolute --git-dir 2>/dev/null)" \
    && common_dir="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" \
    && [[ "$git_dir" != "$common_dir" ]]; then
    linked_worktree=1
fi

if [[ "${CI:-}" != "true" ]]; then
    "$root/scripts/seed-worktree-target.sh" --if-needed "$root"
fi

if [[ -z "$cef_cache" && "${CI:-}" != "true" ]]; then
    case "$(uname -s)" in
        Darwin) cef_cache="$HOME/Library/Caches/Vmux/cef-sdk" ;;
        *) cef_cache="${XDG_CACHE_HOME:-$HOME/.cache}/vmux/cef-sdk" ;;
    esac
fi

if sccache_bin="$(command -v sccache 2>/dev/null)"; then
    export CMAKE_C_COMPILER_LAUNCHER="${CMAKE_C_COMPILER_LAUNCHER:-$sccache_bin}"
    export CMAKE_CXX_COMPILER_LAUNCHER="${CMAKE_CXX_COMPILER_LAUNCHER:-$sccache_bin}"
    if [[ "${CI:-}" != "true" && "$linked_worktree" == "1" ]]; then
        export RUSTC_WRAPPER="${RUSTC_WRAPPER:-$sccache_bin}"
        if [[ "$RUSTC_WRAPPER" == "$sccache_bin" ]]; then
            export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
        fi
    fi
fi

if [[ -n "$cef_cache" ]]; then
    mkdir -p "$cef_cache"
    exec env CEF_PATH="$cef_cache" "$cargo_bin" "$@"
fi

exec env -u CEF_PATH "$cargo_bin" "$@"
