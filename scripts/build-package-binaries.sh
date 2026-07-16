#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/cargo-target-paths.sh"

release_dir="$(vmux_cargo_profile_dir "$ROOT" release)"
helper_dir="$(vmux_cargo_profile_dir "$ROOT" cef-helper)"

cd "$ROOT"
"$ROOT/scripts/cargo-with-cef-cache.sh" build -p vmux_cli --release
"$ROOT/scripts/cargo-with-cef-cache.sh" build -p vmux_desktop -p vmux_service --release
"$ROOT/scripts/cargo-with-cef-cache.sh" build -p bevy_cef_debug_render_process --profile cef-helper
cp -f "$helper_dir/bevy_cef_debug_render_process" "$release_dir/bevy_cef_debug_render_process"
cp -f "$release_dir/vmux_service" "$release_dir/Vmux Service"
