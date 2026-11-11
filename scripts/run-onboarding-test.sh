#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
mode="${1:-clean}"
case "$mode" in
    clean|env) ;;
    *) echo "Usage: $0 [clean|env] [absolute-data-directory]" >&2; exit 2 ;;
esac
data="${2:-$(mktemp -d "${TMPDIR:-/tmp}/vmux-onboarding-$mode.XXXXXX")}"
case "$data" in
    /*) ;;
    *) echo "The test data directory must be absolute." >&2; exit 2 ;;
esac
mkdir -p "$data/profiles/onboarding-$mode/home" "$data/config/projects"
echo "Onboarding test data: $data"
export VMUX_DATA_DIR="$data"
export VMUX_PROFILE="onboarding-$mode"
export VMUX_TEST_ENV_KEYS=0
if [[ "$mode" == env ]]; then
    export VMUX_TEST_ENV_KEYS=1
fi
exec "$root/scripts/run-dev.sh"
