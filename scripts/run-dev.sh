#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root/scripts/cargo-target-paths.sh"
debug_dir="$(vmux_cargo_profile_dir "$root" debug)"
stamp="$debug_dir/.vmux-dev-ready"

if [[ "${1:-}" != stop ]] && [[ ! -f "$stamp" || "$(head -n 1 "$stamp")" != "$root" ]]; then
    echo "No debug build for this worktree. Run make build-dev first." >&2
    exit 1
fi
if [[ "${1:-}" != stop ]] && ! (tail -n +2 "$stamp" | shasum -a 256 --check --status); then
    echo "Debug artifacts changed after the build. Run make build-dev again." >&2
    exit 1
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
exec env -u CEF_PATH DYLD_LIBRARY_PATH="$dylib_path" \
    VMUX_PROFILE="${VMUX_PROFILE:-personal}" VMUX_TEST="${VMUX_TEST:-}" "$debug_dir/vmux_desktop"
