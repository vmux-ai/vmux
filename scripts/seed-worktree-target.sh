#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root/scripts/target-cache-common.sh"

if_needed=0
if [[ "${1:-}" == "--if-needed" ]]; then
    if_needed=1
    shift
fi

if [[ "$#" -gt 1 ]]; then
    echo "usage: $0 [--if-needed] [worktree-path]" >&2
    exit 1
fi

skip_or_fail() {
    if [[ "$if_needed" == "1" ]]; then
        exit 0
    fi
    echo "$1" >&2
    exit 1
}

if ! common_dir="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)"; then
    skip_or_fail "not inside a git worktree"
fi

main_root="$(dirname "$common_dir")"
destination_root="${1:-$(git rev-parse --show-toplevel)}"
destination_root="$(cd "$destination_root" && pwd)"
target_dir_setting="${CARGO_TARGET_DIR:-target}"
if [[ -n "${VMUX_TARGET_DESTINATION:-}" ]]; then
    destination_target="$VMUX_TARGET_DESTINATION"
elif [[ "$target_dir_setting" == /* ]]; then
    destination_target="$target_dir_setting"
else
    destination_target="$destination_root/$target_dir_setting"
fi
destination_parent="$(dirname "$destination_target")"
mkdir -p "$destination_parent"
destination_parent="$(cd "$destination_parent" && pwd)"
destination_target="$destination_parent/$(basename "$destination_target")"
target_seed_key="${VMUX_TARGET_SEED_KEY:-}"
source_target="${VMUX_TARGET_SEED:-}"
staging_target=""

cleanup() {
    vmux_target_lock_release 9
    vmux_target_lock_release 8
    if [[ -n "$staging_target" && -e "$staging_target" ]]; then
        rm -rf -- "$staging_target"
    fi
}
trap cleanup EXIT

vmux_target_lock_acquire "$destination_target" "$root" 8

if [[ -e "$destination_target" || -L "$destination_target" ]]; then
    skip_or_fail "target destination already exists: $destination_target"
fi

if [[ -z "$source_target" && -n "$target_seed_key" ]]; then
    newest_marker_mtime=0
    while IFS= read -r line; do
        if [[ "$line" != worktree\ * ]]; then
            continue
        fi
        candidate_root="${line#worktree }"
        if [[ "$target_dir_setting" == /* ]]; then
            candidate_target="$target_dir_setting"
        else
            candidate_target="$candidate_root/$target_dir_setting"
        fi
        marker="$candidate_target/.vmux-seed/$target_seed_key"
        if [[ "$candidate_target" == "$destination_target" || -L "$candidate_target" || ! -f "$marker" ]]; then
            continue
        fi
        case "$(uname -s)" in
            Darwin) marker_mtime="$(stat -f '%m' "$marker")" ;;
            *) marker_mtime="$(stat -c '%Y' "$marker")" ;;
        esac
        if [[ "$marker_mtime" -gt "$newest_marker_mtime" ]]; then
            source_target="$candidate_target"
            newest_marker_mtime="$marker_mtime"
        fi
    done < <(git worktree list --porcelain)
    if [[ -z "$source_target" ]]; then
        skip_or_fail "compatible target seed not found"
    fi
fi

source_target="${source_target:-$main_root/target}"
if [[ ! -d "$source_target" || -L "$source_target" ]]; then
    skip_or_fail "target seed not found or unsafe: $source_target"
fi
source_target="$(cd "$source_target" && pwd)"

if [[ "$source_target" == "$destination_target" ]]; then
    skip_or_fail "target seed and destination are identical: $source_target"
fi

if [[ -n "${VMUX_TARGET_SEED:-}" ]]; then
    source_lock_mode="wait"
else
    source_lock_mode="try"
fi
if ! vmux_target_lock_acquire "$source_target" "$root" 9 "$source_lock_mode"; then
    skip_or_fail "compatible target seed busy: $source_target"
fi

if [[ -n "$target_seed_key" && ! -f "$source_target/.vmux-seed/$target_seed_key" ]]; then
    skip_or_fail "target seed became incompatible: $source_target"
fi

staging_target="$destination_parent/.vmux-target-seed.$$.$RANDOM"
if [[ -e "$staging_target" || -L "$staging_target" ]]; then
    skip_or_fail "target staging path already exists: $staging_target"
fi

case "$(uname -s)" in
    Darwin)
        source_device="$(df "$source_target" | awk 'END { print $1 }')"
        destination_device="$(df "$destination_parent" | awk 'END { print $1 }')"
        if [[ "$source_device" != "$destination_device" ]] || ! mount | awk -v device="$source_device" '$1 == device && $0 ~ /\(apfs,/ { found = 1 } END { exit !found }'; then
            echo "target seed requires source and destination on the same APFS filesystem" >&2
            exit 1
        fi
        cp -cR "$source_target" "$staging_target"
        ;;
    Linux)
        cp --reflink=always -a "$source_target" "$staging_target"
        ;;
    *)
        echo "target seed unsupported on $(uname -s)" >&2
        exit 1
        ;;
esac

vmux_target_lock_release 9

find "$staging_target" -type d \( -name incremental -o -path '*/build/cef-dll-sys-*' -o -path '*/.fingerprint/cef-dll-sys-*' \) -prune -exec rm -rf -- {} \;
find "$staging_target" -type f \( -name 'cef_dll_sys-*' -o -name 'libcef_dll_sys-*' \) -delete

mv -n "$staging_target" "$destination_target"
if [[ -e "$staging_target" ]]; then
    skip_or_fail "target destination appeared during seed: $destination_target"
fi
staging_target=""

echo "seeded target: $source_target -> $destination_target"
