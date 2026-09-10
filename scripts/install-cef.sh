#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT="${1:-$HOME/.local/share}"
CARGO_BIN="${CARGO_BIN:-$(command -v cargo 2>/dev/null || echo "$HOME/.cargo/bin/cargo")}"
EXPORT_CEF_BIN="${EXPORT_CEF_BIN:-$(command -v export-cef-dir 2>/dev/null || echo "$HOME/.cargo/bin/export-cef-dir")}"

install_stock_cef() {
    local version
    version="$(awk -F'"' '/^name = "cef"$/{getline; print $2; exit}' "$ROOT/Cargo.lock")"
    if [[ -z "$version" ]]; then
        echo "could not resolve cef crate version from Cargo.lock" >&2
        exit 1
    fi
    "$CARGO_BIN" install "export-cef-dir@$version" --force
    "$EXPORT_CEF_BIN" --force "$OUTPUT"
}

if [[ "$(uname -s)" != "Darwin" ]]; then
    install_stock_cef
    exit
fi

if [[ "$(uname -m)" != "arm64" ]]; then
    echo "custom CEF is only published for macOS ARM64" >&2
    exit 1
fi

repository="$("$ROOT/scripts/cef-manifest.sh" artifact_repository)"
tag="$("$ROOT/scripts/cef-manifest.sh" artifact_tag)"
name="$("$ROOT/scripts/cef-manifest.sh" artifact_name)"
expected="$("$ROOT/scripts/cef-manifest.sh" artifact_sha256)"

if [[ ! "$expected" =~ ^[0-9a-f]{64}$ ]]; then
    echo "cef/manifest.toml does not contain a published artifact checksum" >&2
    exit 1
fi

temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
archive="$temporary/$name"

if [[ -n "${VMUX_CEF_ARCHIVE:-}" ]]; then
    cp "$VMUX_CEF_ARCHIVE" "$archive"
else
    url="${VMUX_CEF_URL:-https://github.com/$repository/releases/download/$tag/$name}"
    curl --retry 5 --retry-delay 2 --retry-max-time 300 -fsSL -o "$archive" "$url"
fi

actual="$(shasum -a 256 "$archive" | awk '{print $1}')"
if [[ "$actual" != "$expected" ]]; then
    echo "CEF checksum mismatch: expected $expected, got $actual" >&2
    exit 1
fi

mkdir -p "$temporary/extracted"
tar -xzf "$archive" -C "$temporary/extracted"
framework="$temporary/extracted/Chromium Embedded Framework.framework"
binary="$framework/Chromium Embedded Framework"

if [[ ! -x "$binary" ]]; then
    echo "CEF archive does not contain the expected framework" >&2
    exit 1
fi
if ! nm -gU "$binary" | awk '$NF == "_cef_set_os_crypt_keys" { found = 1 } END { exit !found }'; then
    echo "CEF archive does not export cef_set_os_crypt_keys" >&2
    exit 1
fi

mkdir -p "$OUTPUT"
destination="$OUTPUT/Chromium Embedded Framework.framework"
rm -rf "$destination"
mv "$framework" "$destination"
echo "Installed $name to $destination"
