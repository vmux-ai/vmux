#!/usr/bin/env bash
set -euo pipefail

build_profile="${1:?build profile required}"
checkout="${2:?checkout path required}"
product_name="${VMUX_HOST_NAME:-Vmux}"
case "$build_profile" in
    release) title="$product_name" ;;
    dev|local)
        source_label="$(git -C "$checkout" symbolic-ref --quiet --short HEAD 2>/dev/null \
            || git -C "$checkout" rev-parse --short=7 HEAD 2>/dev/null \
            || printf 'unknown')"
        if [[ "$build_profile" == dev ]]; then
            title="$product_name Dev ($source_label)"
        else
            title="$product_name ($source_label)"
        fi
        ;;
    *) title="$product_name ($build_profile)" ;;
esac

if [[ -n "${3:-}" ]]; then
    python3 - "$3" "$title" <<'PLIST'
import plistlib
import sys
with open(sys.argv[1], 'rb') as source:
    data = plistlib.load(source)
data['CFBundleDisplayName'] = sys.argv[2]
data['CFBundleName'] = sys.argv[2]
with open(sys.argv[1], 'wb') as destination:
    plistlib.dump(data, destination, sort_keys=False)
PLIST
fi
printf '%s\n' "$title"
