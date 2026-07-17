#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
: "${CHROME_BIN:?set CHROME_BIN to a Chrome for Testing or Chromium 148 executable}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
case "$TARGET_DIR" in
  /*) ;;
  *) TARGET_DIR="$ROOT/$TARGET_DIR" ;;
esac
DEBUG_DIR="$TARGET_DIR/debug"
DEFAULT_VMUX_BIN="$DEBUG_DIR/vmux_desktop"
VMUX_BIN="${VMUX_BIN:-$DEFAULT_VMUX_BIN}"
case "$VMUX_BIN" in
  /*) ;;
  *) VMUX_BIN="$ROOT/$VMUX_BIN" ;;
esac
OUT="$TARGET_DIR/extension-conformance"
mkdir -p "$OUT"
HARNESS="$DEBUG_DIR/vmux-extension-conformance"
BUILD_PROFILE="${VMUX_CONFORMANCE_BUILD_PROFILE:-${VMUX_BUILD_PROFILE:-dev}}"

case "$VMUX_BIN" in
  "$DEFAULT_VMUX_BIN")
    env -u CEF_PATH cargo build \
      -p vmux_browser -p vmux_desktop -p vmux_service \
      --features vmux_desktop/conformance
    ;;
  *)
    : "${VMUX_CONFORMANCE_BUILD_PROFILE:?set VMUX_CONFORMANCE_BUILD_PROFILE for a custom VMUX_BIN}"
    BUILD_PROFILE="$VMUX_CONFORMANCE_BUILD_PROFILE"
    env -u CEF_PATH cargo build -p vmux_browser --bin vmux-extension-conformance
    ;;
esac

"$HARNESS" capture --target chrome --browser "$CHROME_BIN" --output "$OUT/chrome.json"
VMUX_CONFORMANCE_BUILD_PROFILE="$BUILD_PROFILE" \
  "$HARNESS" capture --target vmux --browser "$VMUX_BIN" --output "$OUT/vmux.json"
"$HARNESS" compare --baseline "$OUT/chrome.json" --candidate "$OUT/vmux.json"
