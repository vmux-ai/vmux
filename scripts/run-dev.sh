#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
source "$root/scripts/cargo-target-paths.sh"
debug_dir="$(vmux_cargo_profile_dir "$root" debug)"
stamp="$debug_dir/.vmux-dev-ready"

if [[ "${1:-}" != stop ]]; then
    ready=false
    if [[ -f "$stamp" && "$(head -n 1 "$stamp")" == "$root" ]] \
        && (tail -n +2 "$stamp" | shasum -a 256 --check --status) \
        && { [[ "$(uname -s)" != Darwin ]] || [[ -d "$debug_dir/Vmux Dev.app" ]]; }; then
        ready=true
    fi
    if [[ "$ready" != true ]]; then
        if [[ "${1:-}" == build-if-needed ]]; then
            "${MAKE:-make}" -C "$root" build-dev
            exec "$root/scripts/run-dev.sh"
        fi
        echo "No completed debug build for this worktree. Run make build-dev first." >&2
        exit 1
    fi
fi

rust_target_libdir="$(rustc --print target-libdir)"
dylib_path="$rust_target_libdir:$debug_dir/deps"
if [[ -n "${DYLD_LIBRARY_PATH:-}" ]]; then
    dylib_path="$dylib_path:$DYLD_LIBRARY_PATH"
fi
if [[ "${1:-}" == stop ]]; then
    exec env -u CEF_PATH DYLD_LIBRARY_PATH="$dylib_path" \
        VMUX_PROFILE="${VMUX_PROFILE:-personal}" "$debug_dir/vmux" service stop
fi
if [[ "$(uname -s)" == Darwin ]]; then
    app="$debug_dir/Vmux Dev.app"
    [[ -d "$app" ]] || { echo "Debug app missing. Run make build-dev." >&2; exit 1; }
    codesign --verify --strict "$app"
    launch_args=(--env "VMUX_PROFILE=${VMUX_PROFILE:-personal}" --env "VMUX_TEST=${VMUX_TEST:-}")
    if [[ -n "${VMUX_DATA_DIR:-}" ]]; then
        launch_args+=(--env "VMUX_DATA_DIR=$VMUX_DATA_DIR")
    fi
    exec open "${launch_args[@]}" "$app"
fi
exec env -u CEF_PATH DYLD_LIBRARY_PATH="$dylib_path" \
    VMUX_PROFILE="${VMUX_PROFILE:-personal}" VMUX_TEST="${VMUX_TEST:-}" "$debug_dir/vmux_desktop"
