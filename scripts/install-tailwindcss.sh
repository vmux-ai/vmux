#!/usr/bin/env bash
set -euo pipefail

VERSION="${TAILWINDCSS_VERSION:-4.2.4}"
INSTALL_DIR="${TAILWINDCSS_INSTALL_DIR:-/usr/local/bin}"

if command -v tailwindcss >/dev/null 2>&1; then
    VERSION_OUTPUT="$(tailwindcss --version 2>&1 || true)"
    FIRST_LINE="${VERSION_OUTPUT%%$'\n'*}"
    if [[ "$FIRST_LINE" == *"v${VERSION}"* ]]; then
        exit 0
    fi
fi

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
        ASSET="tailwindcss-linux-x64"
        ;;
    Linux-aarch64 | Linux-arm64)
        ASSET="tailwindcss-linux-arm64"
        ;;
    Darwin-x86_64)
        ASSET="tailwindcss-macos-x64"
        ;;
    Darwin-arm64)
        ASSET="tailwindcss-macos-arm64"
        ;;
    *)
        echo "unsupported platform for tailwindcss: $(uname -s)-$(uname -m)" >&2
        exit 1
        ;;
esac

TMP="$(mktemp -t tailwindcss.XXXXXX)"
trap 'rm -f "$TMP"' EXIT

curl -fsSL -o "$TMP" "https://github.com/tailwindlabs/tailwindcss/releases/download/v${VERSION}/${ASSET}"
chmod +x "$TMP"
mkdir -p "$INSTALL_DIR"

if [[ -w "$INSTALL_DIR" ]]; then
    mv "$TMP" "$INSTALL_DIR/tailwindcss"
else
    sudo install -m 0755 "$TMP" "$INSTALL_DIR/tailwindcss"
fi
