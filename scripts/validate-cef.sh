#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FRAMEWORK="${1:-${CEF_FRAMEWORK_DIR:-${HOME}/.local/share/Chromium Embedded Framework.framework}}"
BINARY="$FRAMEWORK/Chromium Embedded Framework"
CEF_VERSION="$("$ROOT/scripts/cef-manifest.sh" cef_version)"

IFS=. read -r major minor patch extra <<< "$CEF_VERSION"
if [[ -n "${extra:-}" || ! "$major" =~ ^[0-9]+$ || ! "$minor" =~ ^[0-9]+$ || ! "$patch" =~ ^[0-9]+$ ]]; then
    echo "Invalid CEF version in cef/manifest.toml: $CEF_VERSION" >&2
    exit 2
fi

EXPECTED_DYLIB_VERSION="$((10#$major * 10 + 10#$minor)).0.$((10#$patch))"

if [[ ! -x "$BINARY" ]]; then
    echo "CEF framework binary missing: $BINARY" >&2
    exit 1
fi

ACTUAL_DYLIB_VERSION="$(otool -L "$BINARY" | awk '
    /compatibility version/ {
        sub(/^.*compatibility version /, "")
        sub(/,.*$/, "")
        print
        exit
    }
')"

if [[ "$ACTUAL_DYLIB_VERSION" != "$EXPECTED_DYLIB_VERSION" ]]; then
    echo "CEF framework version mismatch: expected $CEF_VERSION ($EXPECTED_DYLIB_VERSION), got ${ACTUAL_DYLIB_VERSION:-unknown} at $FRAMEWORK" >&2
    exit 1
fi

if ! nm -gU "$BINARY" | awk '$NF == "_cef_set_os_crypt_keys" { found = 1 } END { exit !found }'; then
    echo "CEF $CEF_VERSION at $FRAMEWORK does not export cef_set_os_crypt_keys" >&2
    exit 1
fi

echo "CEF $CEF_VERSION validated at $FRAMEWORK"
