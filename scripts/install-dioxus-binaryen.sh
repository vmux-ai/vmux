#!/usr/bin/env bash
set -euo pipefail

VERSION="${BINARYEN_VERSION:-127}"

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
        ASSET="binaryen-version_${VERSION}-x86_64-linux.tar.gz"
        ;;
    Linux-aarch64 | Linux-arm64)
        ASSET="binaryen-version_${VERSION}-aarch64-linux.tar.gz"
        ;;
    Darwin-x86_64)
        ASSET="binaryen-version_${VERSION}-x86_64-macos.tar.gz"
        ;;
    Darwin-arm64)
        ASSET="binaryen-version_${VERSION}-arm64-macos.tar.gz"
        ;;
    *)
        echo "unsupported platform for binaryen: $(uname -s)-$(uname -m)" >&2
        exit 1
        ;;
esac

if [[ -n "${DX_HOME:-}" ]]; then
    DX_DIR="$DX_HOME"
elif [[ "$(uname -s)" == "Darwin" ]]; then
    DX_DIR="$HOME/.dx"
else
    DX_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/.dx"
fi

INSTALL_DIR="$DX_DIR/tools/binaryen-$VERSION"
BIN="$INSTALL_DIR/bin/wasm-opt"

if [[ -x "$BIN" ]]; then
    VERSION_OUTPUT="$("$BIN" --version 2>&1 || true)"
    if [[ "$VERSION_OUTPUT" == *"version $VERSION"* || "$VERSION_OUTPUT" == *"version_$VERSION"* ]]; then
        exit 0
    fi
fi

TMP_DIR="$(mktemp -d -t binaryen.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

curl --retry 5 --retry-delay 2 --retry-max-time 120 -fsSL -o "$TMP_DIR/binaryen.tar.gz" "https://github.com/WebAssembly/binaryen/releases/download/version_${VERSION}/${ASSET}"
rm -rf "$INSTALL_DIR"
mkdir -p "$INSTALL_DIR"
tar -xzf "$TMP_DIR/binaryen.tar.gz" --strip-components 1 -C "$INSTALL_DIR"
chmod +x "$BIN"

if command -v xattr >/dev/null 2>&1; then
    xattr -dr com.apple.quarantine "$INSTALL_DIR" >/dev/null 2>&1 || true
fi

"$BIN" --version
