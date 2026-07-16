#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$ROOT"
"$ROOT/scripts/cargo-with-cef-cache.sh" build -p vmux_desktop -p vmux_cli -p vmux_service -p bevy_cef_debug_render_process --release
cp -f target/release/vmux_service "target/release/Vmux Service"
