#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
if [[ "${1:-}" != --test && -n "${VMUX_BUILD_STORAGE_BIN:-}" ]]; then
    [[ -x "$VMUX_BUILD_STORAGE_BIN" ]] || { echo "Missing build storage executable" >&2; exit 1; }
    printf '%s\n' "$VMUX_BUILD_STORAGE_BIN"
    exit
fi
cd "$root"
source "$root/scripts/target-cache-common.sh"
cache="$root/.build-tools"
[[ ! -L "$cache" ]] || { echo "Unsafe build tool cache: $cache" >&2; exit 1; }
mkdir -p "$cache"
vmux_target_lock_acquire "$cache" "$root" 9 >&2
temporary=""
trap '[[ -z "$temporary" ]] || rm -rf "$temporary"; vmux_target_lock_release 9' EXIT
compiler="${RUSTC:-rustc}"
source_dir="$root/crates/app/vmux_build/src"
kind=vmux-build
compile_args=(--edition=2024 -C opt-level=1 -C debuginfo=0)
if [[ "${1:-}" == --test ]]; then
    kind=vmux-build-tests
    compile_args+=(--test)
fi
key="$({ "$compiler" -vV; git hash-object "$root/scripts/build-storage.sh" "$source_dir/"*.rs; } | git hash-object --stdin)"
binary="$cache/$kind"
if [[ ! -x "$binary" || ! -f "$binary.key" || "$(<"$binary.key")" != "$key" ]]; then
    temporary="$(mktemp -d "$cache/.compile.XXXXXX")"
    "$compiler" "${compile_args[@]}" "$source_dir/main.rs" -o "$temporary/output"
    mv -f "$temporary/output" "$binary"
    printf '%s\n' "$key" > "$binary.key"
fi
if [[ "${1:-}" == --test ]]; then
    "$binary"
else
    printf '%s\n' "$binary"
fi
