#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${CEF_BUILD_DIR:-${RUNNER_TEMP:-/tmp}/vmux-cef-build}"
DIST_DIR="${CEF_DIST_DIR:-$ROOT/cef/dist}"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
    echo "CEF must be built on macOS ARM64" >&2
    exit 1
fi

available_kib="$(df -Pk "$BUILD_DIR" 2>/dev/null | awk 'NR == 2 {print $4}')"
if [[ -z "$available_kib" ]]; then
    available_kib="$(df -Pk "$(dirname "$BUILD_DIR")" | awk 'NR == 2 {print $4}')"
fi
if (( available_kib < 209715200 )); then
    echo "CEF build requires at least 200 GiB free in $BUILD_DIR" >&2
    exit 1
fi

cef_version="$("$ROOT/scripts/cef-manifest.sh" cef_version)"
cef_commit="$("$ROOT/scripts/cef-manifest.sh" cef_commit)"
chromium_version="$("$ROOT/scripts/cef-manifest.sh" chromium_version)"
patch_revision="$("$ROOT/scripts/cef-manifest.sh" patch_revision)"
artifact_name="$("$ROOT/scripts/cef-manifest.sh" artifact_name)"

mkdir -p "$BUILD_DIR" "$DIST_DIR"
automate="$BUILD_DIR/automate-git.py"
curl --retry 5 --retry-delay 2 --retry-max-time 300 -fsSL \
    -o "$automate" \
    "https://raw.githubusercontent.com/chromiumembedded/cef/$cef_commit/tools/automate/automate-git.py"

python3 "$automate" \
    --download-dir="$BUILD_DIR" \
    --checkout="$cef_commit" \
    --chromium-checkout="refs/tags/$chromium_version" \
    --arm64-build \
    --no-chromium-history \
    --force-clean \
    --no-build \
    --no-distrib

cef_source="$BUILD_DIR/chromium/src/cef"
chromium_source="$BUILD_DIR/chromium/src"
compatibility="$(awk -F"'" '/chromium_checkout/{print $4}' "$cef_source/CHROMIUM_BUILD_COMPATIBILITY.txt")"
if [[ "$compatibility" != "refs/tags/$chromium_version" ]]; then
    echo "CEF expects $compatibility, not refs/tags/$chromium_version" >&2
    exit 1
fi

git -C "$cef_source" apply --check "$ROOT/cef/patches/cef-safe-storage.patch"
git -C "$cef_source" apply "$ROOT/cef/patches/cef-safe-storage.patch"
git -C "$chromium_source" apply --check "$ROOT/cef/patches/chromium-safe-storage.patch"
git -C "$chromium_source" apply "$ROOT/cef/patches/chromium-safe-storage.patch"
git -C "$cef_source" diff --check
git -C "$chromium_source" diff --check

export GN_DEFINES="is_official_build=true"
export CEF_ARCHIVE_FORMAT="tar.bz2"
python3 "$automate" \
    --download-dir="$BUILD_DIR" \
    --checkout="$cef_commit" \
    --chromium-checkout="refs/tags/$chromium_version" \
    --arm64-build \
    --no-chromium-history \
    --no-update \
    --force-build \
    --force-distrib \
    --minimal-distrib-only \
    --no-debug-build \
    --build-target=cefsimple \
    --with-pgo-profiles

source_archive="$(find "$cef_source/binary_distrib" -maxdepth 1 -type f -name 'cef_binary_*_macosarm64_minimal.tar.bz2' -print | sort | tail -1)"
if [[ -z "$source_archive" ]]; then
    echo "CEF minimal distribution was not produced" >&2
    exit 1
fi

temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
tar -xjf "$source_archive" -C "$temporary"
framework="$(find "$temporary" -type d -path '*/Release/Chromium Embedded Framework.framework' -print -quit)"
if [[ -z "$framework" ]]; then
    echo "CEF distribution does not contain the release framework" >&2
    exit 1
fi
if ! nm -gU "$framework/Chromium Embedded Framework" | awk '$NF == "_cef_set_os_crypt_keys" { found = 1 } END { exit !found }'; then
    echo "built CEF does not export cef_set_os_crypt_keys" >&2
    exit 1
fi

artifact="$DIST_DIR/$artifact_name"
COPYFILE_DISABLE=1 tar -czf "$artifact" -C "$(dirname "$framework")" "$(basename "$framework")"
checksum="$(shasum -a 256 "$artifact" | awk '{print $1}')"
printf '%s  %s\n' "$checksum" "$artifact_name" > "$artifact.sha256"
printf '{\n  "cef_version": "%s",\n  "cef_commit": "%s",\n  "chromium_version": "%s",\n  "patch_revision": %s,\n  "artifact_sha256": "%s",\n  "repository_commit": "%s",\n  "xcode": "%s"\n}\n' \
    "$cef_version" \
    "$cef_commit" \
    "$chromium_version" \
    "$patch_revision" \
    "$checksum" \
    "$(git -C "$ROOT" rev-parse HEAD)" \
    "$(xcodebuild -version | tr '\n' ' ')" \
    > "$artifact.metadata.json"

echo "$checksum"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    {
        echo "artifact=$artifact"
        echo "checksum=$checksum"
        echo "checksum_file=$artifact.sha256"
        echo "metadata=$artifact.metadata.json"
    } >> "$GITHUB_OUTPUT"
fi
