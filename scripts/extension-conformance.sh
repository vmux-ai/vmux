#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
: "${CHROME_BIN:?set CHROME_BIN to a Chrome for Testing or Chromium 148 executable}"
VMUX_BIN="${VMUX_BIN:-$ROOT/target/debug/vmux_desktop}"
OUT="$ROOT/target/extension-conformance"
mkdir -p "$OUT"

cargo run -p vmux_browser --bin vmux-extension-conformance -- capture --target chrome --browser "$CHROME_BIN" --output "$OUT/chrome.json"
cargo run -p vmux_browser --bin vmux-extension-conformance -- capture --target vmux --browser "$VMUX_BIN" --output "$OUT/vmux.json"
cargo run -p vmux_browser --bin vmux-extension-conformance -- compare --baseline "$OUT/chrome.json" --candidate "$OUT/vmux.json"
