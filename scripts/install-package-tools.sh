#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
. "$ROOT/tool-versions.env"

CARGO_BIN="${CARGO_BIN:-$(command -v cargo 2>/dev/null || echo "$HOME/.cargo/bin/cargo")}"
CARGO_PACKAGER_BIN="${CARGO_PACKAGER_BIN:-$(command -v cargo-packager 2>/dev/null || echo "$HOME/.cargo/bin/cargo-packager")}"
BEVY_CEF_BUNDLE_APP_BIN="${BEVY_CEF_BUNDLE_APP_BIN:-$(command -v bevy_cef_bundle_app 2>/dev/null || echo "$HOME/.cargo/bin/bevy_cef_bundle_app")}"

packager_version="$("$CARGO_PACKAGER_BIN" --version 2>/dev/null | awk '{print $2}' || true)"
if [[ "$packager_version" != "$CARGO_PACKAGER_VERSION" ]]; then
    "$CARGO_BIN" install --path "$ROOT/patches/cargo-packager" --locked --force
fi

bundle_version="$("$BEVY_CEF_BUNDLE_APP_BIN" --version 2>/dev/null | awk '{print $2}' || true)"
if [[ "$bundle_version" != "$BEVY_CEF_BUNDLE_APP_VERSION" ]]; then
    "$CARGO_BIN" install bevy_cef_bundle_app --locked --version "$BEVY_CEF_BUNDLE_APP_VERSION"
fi
