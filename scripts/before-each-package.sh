#!/usr/bin/env bash
set -euo pipefail

# cargo-packager `before-each-package-command` hook.
# Runs once per format (CARGO_PACKAGER_FORMAT in {app, dmg}).
#
# - `app` pass: nothing meaningful to do here -- the .app doesn't exist yet
#   (about to be built). inject-cef.sh self-skips for non-dmg formats.
# - `dmg` pass: the .app exists. Inject CEF, then sign + notarize so the
#   DMG bundles a release-ready .app.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$ROOT/scripts/inject-cef.sh"

if [[ "${CARGO_PACKAGER_FORMAT:-}" == "dmg" ]]; then
    APP_BUNDLE="${VMUX_APP_BUNDLE:-$ROOT/target/release/Vmux.app}" \
        "$ROOT/scripts/sign-and-notarize.sh"
fi
