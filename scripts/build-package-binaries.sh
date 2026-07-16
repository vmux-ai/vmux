#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$ROOT"
"$ROOT/scripts/cargo-with-cef-cache.sh" build -p vmux_cli --release
"$ROOT/scripts/cargo-with-cef-cache.sh" build -p vmux_desktop -p vmux_service --release
"$ROOT/scripts/cargo-with-cef-cache.sh" build -p bevy_cef_debug_render_process --profile cef-helper
cp -f target/cef-helper/bevy_cef_debug_render_process target/release/bevy_cef_debug_render_process
cp -f target/release/vmux_service "target/release/Vmux Service"
