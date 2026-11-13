#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root/scripts/cargo-target-paths.sh"
binary="${VMUX_VM_BINARY:-$(vmux_cargo_profile_dir "$root" debug)/vmux_vm}"
[[ "$(uname -s)" == Darwin ]] || exit 0
codesign --force --sign "${APPLE_SIGNING_IDENTITY:--}" --options runtime \
    --identifier ai.vmux.vm \
    --entitlements "$root/crates/util/vmux_vm/hypervisor.entitlements" "$binary"
codesign --verify --strict "$binary"
