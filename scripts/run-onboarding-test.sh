#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
mode="${1:-clean}"
case "$mode" in
    clean|env) ;;
    *) echo "Usage: $0 [clean|env] [absolute-data-directory]" >&2; exit 2 ;;
esac
data="${2:-$(mktemp -d "/tmp/vmux-fresh-$mode.XXXXXX")}"
case "$data" in
    /*) ;;
    *) echo "The test data directory must be absolute." >&2; exit 2 ;;
esac
profile=fresh
if [[ "$mode" == env ]]; then
    profile=fresh-env
fi
mkdir -p "$data/profiles/$profile/home" "$data/config/projects"
echo "Onboarding test data: $data"
unset VMUX_ONBOARDING_FRESH
export VMUX_DATA_DIR="$data"
export VMUX_PROFILE="$profile"
export VMUX_TEST_ENV_KEYS=0
if [[ "$mode" == env ]]; then
    export VMUX_TEST_ENV_KEYS=1
fi
exec "$root/scripts/run-dev.sh"
