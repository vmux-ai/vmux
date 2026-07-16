#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO_BIN:-cargo}"
rustc_bin="${RUSTC:-rustc}"
manifest_paths=()

while IFS= read -r path; do
    manifest_paths+=("$path")
done < <(
    find "$root" \
        \( -path "$root/.git" -o -path "$root/.worktrees" -o -path "$root/target" \) -prune \
        -o -type f \
        \( -name Cargo.toml -o -name Cargo.lock -o -name rust-toolchain -o -name rust-toolchain.toml -o -path '*/.cargo/config' -o -path '*/.cargo/config.toml' \) \
        -print \
        | LC_ALL=C sort
)

{
    printf 'cargo=%s\n' "$("$cargo_bin" -V)"
    printf 'rustc=%s\n' "$("$rustc_bin" -vV)"
    printf 'rustc-wrapper=%s\n' "${RUSTC_WRAPPER:-}"
    printf 'rustc-workspace-wrapper=%s\n' "${RUSTC_WORKSPACE_WRAPPER:-}"
    printf 'cargo-incremental=%s\n' "${CARGO_INCREMENTAL:-}"
    printf 'rustflags=%s\n' "${RUSTFLAGS:-}"
    printf 'cargo-encoded-rustflags=%s\n' "${CARGO_ENCODED_RUSTFLAGS:-}"
    printf 'cargo-build-target=%s\n' "${CARGO_BUILD_TARGET:-}"
    printf 'macosx-deployment-target=%s\n' "${MACOSX_DEPLOYMENT_TARGET:-}"
    for path in "${manifest_paths[@]}"; do
        printf 'file=%s\n' "${path#"$root/"}"
    done
    git -C "$root" hash-object "${manifest_paths[@]}"
} | git -C "$root" hash-object --stdin
