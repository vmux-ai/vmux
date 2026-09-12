#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KEY="${1:-}"

if [[ -z "$KEY" ]]; then
    echo "Usage: $0 <key>" >&2
    exit 2
fi

awk -F'[[:space:]]*=[[:space:]]*' -v key="$KEY" '
    $1 == key {
        value = $2
        sub(/^"/, "", value)
        sub(/"$/, "", value)
        print value
        found = 1
        exit
    }
    END {
        if (!found) {
            exit 1
        }
    }
' "$ROOT/cef/manifest.toml"
