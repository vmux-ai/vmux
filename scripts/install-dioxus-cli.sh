#!/usr/bin/env bash
set -euo pipefail

VERSION="${DIOXUS_CLI_VERSION:-0.7.9}"
INSTALL_DIR="${DIOXUS_CLI_INSTALL_DIR:-$HOME/.local/bin}"

if command -v dx >/dev/null 2>&1; then
    VERSION_OUTPUT="$(dx --version 2>&1 || true)"
    if [[ "$VERSION_OUTPUT" == "dioxus $VERSION"* ]]; then
        exit 0
    fi
fi

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
        ASSET="dx-x86_64-unknown-linux-gnu.tar.gz"
        ;;
    Linux-aarch64 | Linux-arm64)
        ASSET="dx-aarch64-unknown-linux-gnu.tar.gz"
        ;;
    Darwin-x86_64)
        ASSET="dx-x86_64-apple-darwin.tar.gz"
        ;;
    Darwin-arm64)
        ASSET="dx-aarch64-apple-darwin.tar.gz"
        ;;
    *)
        echo "unsupported platform for dioxus-cli: $(uname -s)-$(uname -m)" >&2
        exit 1
        ;;
esac

TMP_DIR="$(mktemp -d -t dioxus-cli.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

BASE_URL="https://github.com/DioxusLabs/dioxus/releases/download/v${VERSION}"
curl --retry 5 --retry-delay 2 --retry-max-time 120 -fsSL -o "$TMP_DIR/$ASSET" "$BASE_URL/$ASSET"
curl --retry 5 --retry-delay 2 --retry-max-time 120 -fsSL -o "$TMP_DIR/checksums.sha256" "$BASE_URL/${ASSET%.tar.gz}.sha256"

if command -v sha256sum >/dev/null 2>&1; then
    CHECKSUM=(sha256sum -c -)
elif command -v shasum >/dev/null 2>&1; then
    CHECKSUM=(shasum -a 256 -c -)
else
    echo "sha256sum or shasum is required" >&2
    exit 1
fi

(
    cd "$TMP_DIR"
    grep "  $ASSET$" checksums.sha256 | "${CHECKSUM[@]}"
)

tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"
mkdir -p "$INSTALL_DIR"
install -m 0755 "$TMP_DIR/dx" "$INSTALL_DIR/dx"

if command -v xattr >/dev/null 2>&1; then
    xattr -d com.apple.quarantine "$INSTALL_DIR/dx" >/dev/null 2>&1 || true
fi

"$INSTALL_DIR/dx" --version
