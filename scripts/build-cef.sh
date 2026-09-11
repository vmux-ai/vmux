#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${CEF_BUILD_DIR:-${RUNNER_TEMP:-/tmp}/vmux-cef-build}"
DIST_DIR="${CEF_DIST_DIR:-$ROOT/cef/dist}"
ORIGINAL_PATH="$PATH"

create_reproducible_archive() {
    local source="$1"
    local destination="$2"
    /usr/bin/python3 - "$source" "$destination" <<'PY'
import gzip
import pathlib
import sys
import tarfile

source = pathlib.Path(sys.argv[1])
destination = pathlib.Path(sys.argv[2])
root = source.parent
paths = [source]
paths.extend(sorted(source.rglob("*"), key=lambda path: path.relative_to(root).as_posix()))

with destination.open("wb") as output:
    with gzip.GzipFile(filename="", mode="wb", fileobj=output, compresslevel=9, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
            for path in paths:
                name = path.relative_to(root).as_posix()
                info = archive.gettarinfo(str(path), name)
                info.uid = 0
                info.gid = 0
                info.uname = ""
                info.gname = ""
                info.mtime = 0
                if info.isdir():
                    info.mode = 0o755
                elif info.issym():
                    info.mode = 0o777
                elif info.isfile():
                    info.mode = 0o755 if info.mode & 0o111 else 0o644
                if info.isfile():
                    with path.open("rb") as file:
                        archive.addfile(info, file)
                else:
                    archive.addfile(info)
PY
}

apply_patch_once() {
    local repository="$1"
    local patch="$2"
    if git -C "$repository" apply --reverse --check "$patch" 2>/dev/null; then
        return
    fi
    git -C "$repository" apply --check "$patch"
    git -C "$repository" apply "$patch"
}

unapply_patch_once() {
    local repository="$1"
    local patch="$2"
    if git -C "$repository" apply --reverse --check "$patch" 2>/dev/null; then
        git -C "$repository" apply --reverse "$patch"
    fi
}

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
    echo "CEF must be built on macOS ARM64" >&2
    exit 1
fi
if ! command -v git-lfs >/dev/null 2>&1; then
    echo "CEF build requires git-lfs" >&2
    exit 1
fi
export GIT_LFS_SKIP_SMUDGE=1

mkdir -p "$BUILD_DIR" "$DIST_DIR"
available_kib="$(df -Pk "$BUILD_DIR" | awk 'NR == 2 {print $4}')"
existing_kib="$(du -sk "$BUILD_DIR" | awk '{print $1}')"
if (( available_kib + existing_kib < 199229440 )); then
    echo "CEF build requires at least 190 GiB free in $BUILD_DIR" >&2
    exit 1
fi

cef_version="$("$ROOT/scripts/cef-manifest.sh" cef_version)"
cef_commit="$("$ROOT/scripts/cef-manifest.sh" cef_commit)"
chromium_version="$("$ROOT/scripts/cef-manifest.sh" chromium_version)"
depot_tools_commit="$("$ROOT/scripts/cef-manifest.sh" depot_tools_commit)"
patch_revision="$("$ROOT/scripts/cef-manifest.sh" patch_revision)"
artifact_name="$("$ROOT/scripts/cef-manifest.sh" artifact_name)"
expected_checksum="$("$ROOT/scripts/cef-manifest.sh" artifact_sha256)"

if [[ -z "$expected_checksum" ]]; then
    if [[ "${CEF_ALLOW_UNPINNED:-0}" != "1" ]]; then
        echo "cef/manifest.toml does not contain an artifact checksum; set CEF_ALLOW_UNPINNED=1 only to bootstrap a new pin" >&2
        exit 1
    fi
elif [[ ! "$expected_checksum" =~ ^[0-9a-f]{64}$ ]]; then
    echo "cef/manifest.toml contains an invalid artifact checksum" >&2
    exit 1
fi

automate="$BUILD_DIR/automate-git.py"
depot_tools="$BUILD_DIR/depot_tools"
curl --retry 5 --retry-delay 2 --retry-max-time 300 -fsSL \
    -o "$automate" \
    "https://raw.githubusercontent.com/chromiumembedded/cef/$cef_commit/tools/automate/automate-git.py"
if [[ ! -d "$depot_tools/.git" ]]; then
    git clone https://chromium.googlesource.com/chromium/tools/depot_tools.git "$depot_tools"
fi
git -C "$depot_tools" fetch origin "$depot_tools_commit"
git -C "$depot_tools" checkout --force "$depot_tools_commit"
source "$depot_tools/bootstrap_python3"
bootstrap_python3
depot_python_dir="$depot_tools/$(cat "$depot_tools/python3_bin_reldir.txt")"
export PATH="$depot_python_dir:$depot_tools:$PATH"
export PYTHONPATH="$depot_tools${PYTHONPATH:+:$PYTHONPATH}"
export VPYTHON_BYPASS="manually managed python not supported by chrome operations"

if [[ "${CEF_RESUME:-0}" == "1" ]]; then
    if [[ ! -d "$BUILD_DIR/chromium/src/.git" || ! -d "$BUILD_DIR/chromium/src/cef" ]]; then
        echo "CEF_RESUME requires an existing Chromium and CEF checkout" >&2
        exit 1
    fi
else
    python3 "$automate" \
        --download-dir="$BUILD_DIR" \
        --depot-tools-dir="$depot_tools" \
        --no-depot-tools-update \
        --checkout="$cef_commit" \
        --chromium-checkout="refs/tags/$chromium_version" \
        --arm64-build \
        --no-chromium-history \
        --with-pgo-profiles \
        --force-config \
        --force-clean \
        --no-build \
        --no-distrib
fi

chromium_source="$BUILD_DIR/chromium/src"
cef_source="$chromium_source/cef"
gclient_marker="$BUILD_DIR/.gclient-$chromium_version"
if [[ ! -f "$gclient_marker" ]]; then
    unapply_patch_once "$cef_source" "$ROOT/cef/patches/cef-safe-storage.patch"
    unapply_patch_once "$chromium_source" "$ROOT/cef/patches/chromium-safe-storage.patch"
    git -C "$chromium_source" config --replace-all remote.origin.fetch \
        "+refs/tags/$chromium_version:refs/tags/$chromium_version"
    (
        cd "$BUILD_DIR/chromium"
        DEPOT_TOOLS_UPDATE=0 gclient sync --nohooks --no-history --shallow \
            --revision "src@refs/tags/$chromium_version"
    )
    touch "$gclient_marker"
fi
hooks_marker="$BUILD_DIR/.gclient-hooks-$chromium_version"
if [[ ! -f "$hooks_marker" ]]; then
    (
        cd "$BUILD_DIR/chromium"
        DEPOT_TOOLS_UPDATE=0 gclient runhooks
    )
    touch "$hooks_marker"
fi
python3 "$chromium_source/build/util/lastchange.py" \
    -o "$chromium_source/build/util/LASTCHANGE"

PATH="$depot_tools:$PATH" python3 \
    "$BUILD_DIR/chromium/src/tools/update_pgo_profiles.py" \
    --target mac-arm \
    update \
    --gs-url-base=chromium-optimization-profiles/pgo_profiles
PATH="$depot_tools:$PATH" python3 \
    "$BUILD_DIR/chromium/src/v8/tools/builtins-pgo/download_profiles.py" \
    download \
    --depot-tools "$depot_tools" \
    --check-v8-revision

compatibility="$(awk -F"'" '/chromium_checkout/{print $4}' "$cef_source/CHROMIUM_BUILD_COMPATIBILITY.txt")"
if [[ "$compatibility" != "refs/tags/$chromium_version" ]]; then
    echo "CEF expects $compatibility, not refs/tags/$chromium_version" >&2
    exit 1
fi

apply_patch_once "$cef_source" "$ROOT/cef/patches/cef-safe-storage.patch"
apply_patch_once "$chromium_source" "$ROOT/cef/patches/chromium-safe-storage.patch"
git -C "$cef_source" diff --check -- BUILD.gn libcef/common/base_impl.cc
git -C "$chromium_source" diff --check -- \
    chrome/browser/browser_process_impl.cc \
    components/os_crypt/async/browser/keychain_key_provider.h \
    components/os_crypt/async/browser/keychain_key_provider.mm

export GN_DEFINES="is_official_build=true"
export CEF_ARCHIVE_FORMAT="tar.bz2"
python3 "$automate" \
    --download-dir="$BUILD_DIR" \
    --depot-tools-dir="$depot_tools" \
    --no-depot-tools-update \
    --checkout="$cef_commit" \
    --chromium-checkout="refs/tags/$chromium_version" \
    --arm64-build \
    --no-chromium-history \
    --no-update \
    --force-build \
    --force-distrib \
    --minimal-distrib-only \
    --no-debug-build \
    --build-target=cefclient \
    --with-pgo-profiles

PATH="$ORIGINAL_PATH"
source_archive="$(/usr/bin/find "$cef_source/binary_distrib" -maxdepth 1 -type f -name 'cef_binary_*_macosarm64_minimal.tar.bz2' -print | /usr/bin/sort | /usr/bin/tail -1)"
if [[ -z "$source_archive" ]]; then
    echo "CEF minimal distribution was not produced" >&2
    exit 1
fi

temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
/usr/bin/tar -xjf "$source_archive" -C "$temporary"
framework="$(/usr/bin/find "$temporary" -type d -path '*/Release/Chromium Embedded Framework.framework' -print -quit)"
if [[ -z "$framework" ]]; then
    echo "CEF distribution does not contain the release framework" >&2
    exit 1
fi
if ! /usr/bin/nm -gU "$framework/Chromium Embedded Framework" | /usr/bin/awk '$NF == "_cef_set_os_crypt_keys" { found = 1 } END { exit !found }'; then
    echo "built CEF does not export cef_set_os_crypt_keys" >&2
    exit 1
fi

artifact="$DIST_DIR/$artifact_name"
create_reproducible_archive "$framework" "$artifact"
checksum="$(/usr/bin/shasum -a 256 "$artifact" | /usr/bin/awk '{print $1}')"
if [[ -n "$expected_checksum" && "$checksum" != "$expected_checksum" ]]; then
    echo "CEF artifact checksum differs from cef/manifest.toml: expected $expected_checksum, got $checksum" >&2
    exit 1
fi
printf '%s  %s\n' "$checksum" "$artifact_name" > "$artifact.sha256"
printf '{\n  "cef_version": "%s",\n  "cef_commit": "%s",\n  "chromium_version": "%s",\n  "depot_tools_commit": "%s",\n  "patch_revision": %s,\n  "artifact_sha256": "%s",\n  "repository_commit": "%s",\n  "xcode": "%s"\n}\n' \
    "$cef_version" \
    "$cef_commit" \
    "$chromium_version" \
    "$depot_tools_commit" \
    "$patch_revision" \
    "$checksum" \
    "$(/usr/bin/git -C "$ROOT" rev-parse HEAD)" \
    "$(/usr/bin/xcodebuild -version | /usr/bin/tr '\n' ' ')" \
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
