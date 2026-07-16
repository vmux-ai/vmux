#!/usr/bin/env bash
set -euo pipefail

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
source_target="${VMUX_TARGET_SEED:-$main_root/target}"
destination_target="$destination_root/target"

if [[ ! -d "$source_target" ]]; then
    skip_or_fail "target seed not found: $source_target"
fi

if [[ "$source_target" == "$destination_target" ]]; then
    skip_or_fail "target seed and destination are identical: $source_target"
fi

if [[ -e "$destination_target" ]]; then
    skip_or_fail "target destination already exists: $destination_target"
fi

complete=0
cleanup() {
    if [[ "$complete" != "1" && -e "$destination_target" ]]; then
        rm -rf "$destination_target"
    fi
}
trap cleanup EXIT

case "$(uname -s)" in
    Darwin)
        source_device="$(df "$source_target" | awk 'END { print $1 }')"
        destination_device="$(df "$destination_root" | awk 'END { print $1 }')"
        if [[ "$source_device" != "$destination_device" ]] || ! mount | awk -v device="$source_device" '$1 == device && $0 ~ /\(apfs,/ { found = 1 } END { exit !found }'; then
            echo "target seed requires source and destination on the same APFS filesystem" >&2
            exit 1
        fi
        cp -cR "$source_target" "$destination_target"
        ;;
    Linux)
        cp --reflink=always -a "$source_target" "$destination_target"
        ;;
    *)
        echo "target seed unsupported on $(uname -s)" >&2
        exit 1
        ;;
esac

while IFS= read -r path; do
    rm -rf "$path"
done < <(find "$destination_target" -type d \( -name incremental -o -path '*/build/cef-dll-sys-*' -o -path '*/.fingerprint/cef-dll-sys-*' \) -prune -print)

find "$destination_target" -type f \( -name 'cef_dll_sys-*' -o -name 'libcef_dll_sys-*' \) -delete

complete=1
echo "seeded target: $source_target -> $destination_target"
