#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
source "$root/scripts/cargo-target-paths.sh"
source "$root/scripts/target-cache-common.sh"
profile="${1:?expected dev, local, release, status, or clean}"
shift
case "$profile" in
    dev|local|release|status|clean) ;;
    *) echo "Unknown build storage profile: $profile" >&2; exit 1 ;;
esac
target="$(vmux_cargo_target_dir "$root")"
mkdir -p "$(dirname "$target")"
target="$(cd "$(dirname "$target")" && pwd -P)/$(basename "$target")"
storage=(python3 "$root/scripts/build-storage.py" --root "$root" --target "$target")

if [[ "$profile" == status ]]; then
    exec "${storage[@]}" status
fi
if [[ "$profile" != clean && ! -e "$target" ]]; then
    "${storage[@]}" check
    if [[ "${CI:-}" != true ]]; then
        export VMUX_TARGET_SEED_KEY="$(CARGO_BIN="${CARGO_BIN:-cargo}" "$root/scripts/target-seed-key.sh")"
        "$root/scripts/seed-worktree-target.sh" --if-needed "$root"
    fi
fi
vmux_target_lock_acquire "$target" "$root" 8
trap 'vmux_target_lock_release 8' EXIT
export VMUX_TARGET_LOCK_TARGET="$target"
export VMUX_TARGET_LOCK_OWNER_PID="$$"
export VMUX_BUILD_STORAGE_TARGET="$target"
export VMUX_BUILD_STORAGE_ROOT="$root"
export CARGO_TARGET_DIR="$target"

if [[ "$profile" == clean ]]; then
    "${storage[@]}" clean
    exit
fi
[[ "$#" -gt 0 ]] || { echo "Missing build command" >&2; exit 1; }
"${storage[@]}" preflight
build_pid=""
forward_signal() {
    if [[ -n "$build_pid" ]] && kill -0 "$build_pid" 2>/dev/null; then
        kill -s "$1" "$build_pid" 2>/dev/null || true
    fi
}
trap 'forward_signal HUP' HUP
trap 'forward_signal INT' INT
trap 'forward_signal TERM' TERM
"$@" &
build_pid=$!
while true; do
    status=0
    wait "$build_pid" || status=$?
    if [[ "$status" -gt 128 ]] && kill -0 "$build_pid" 2>/dev/null; then
        continue
    fi
    break
done
build_pid=""
[[ "$status" == 0 ]] || exit "$status"
"${storage[@]}" finish --profile "$profile"
