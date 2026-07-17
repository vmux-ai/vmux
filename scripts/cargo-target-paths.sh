#!/usr/bin/env bash

vmux_cargo_target_dir() {
    local root="$1"
    local target_dir="${CARGO_TARGET_DIR:-target}"

    if [[ "$target_dir" != /* ]]; then
        target_dir="$root/$target_dir"
    fi
    printf '%s\n' "$target_dir"
}

vmux_cargo_profile_dir() {
    local root="$1"
    local profile="$2"
    local target_dir

    target_dir="$(vmux_cargo_target_dir "$root")"
    if [[ -n "${CARGO_BUILD_TARGET:-}" ]]; then
        target_dir="$target_dir/$CARGO_BUILD_TARGET"
    fi
    printf '%s/%s\n' "$target_dir" "$profile"
}
