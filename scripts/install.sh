#!/bin/sh
set -eu

REPO="vmux-ai/vmux"
APP_NAME="Vmux"
INSTALL_DIR="/Applications"

main() {
  echo "Installing ${APP_NAME}..."
  echo ""

  OS="$(uname -s)"
  if [ "$OS" != "Darwin" ]; then
    echo "Error: ${APP_NAME} currently supports macOS only." >&2
    exit 1
  fi

  if command -v brew >/dev/null 2>&1; then
    echo "Homebrew detected. Installing via cask..."
    brew tap "${REPO}" "https://github.com/${REPO}" 2>/dev/null || true
    brew install --cask vmux
    echo ""
    echo "${APP_NAME} installed successfully via Homebrew."
    return
  fi

  echo "Homebrew not found. Installing from GitHub release..."

  ARCH="$(uname -m)"
  case "$ARCH" in
    arm64|aarch64) ARCH_LABEL="aarch64" ;;
    x86_64)        ARCH_LABEL="x86_64" ;;
    *)
      echo "Error: Unsupported architecture: ${ARCH}" >&2
      exit 1
      ;;
  esac

  LATEST_URL="https://api.github.com/repos/${REPO}/releases/latest"
  RELEASE_JSON="$(curl -fsSL "$LATEST_URL")"
  VERSION="$(printf '%s\n' "$RELEASE_JSON" | sed -n 's/.*"tag_name": "v\([^"]*\)".*/\1/p' | head -n 1)"

  if [ -z "$VERSION" ]; then
    echo "Error: Could not determine latest version." >&2
    exit 1
  fi

  echo "Latest version: v${VERSION}"

  DMG_URL="$(printf '%s\n' "$RELEASE_JSON" | sed -n "s|.*\"browser_download_url\": \"\([^\"]*/${APP_NAME}_[^\"]*_${ARCH_LABEL}\\.dmg\)\".*|\1|p" | head -n 1)"
  if [ -z "$DMG_URL" ]; then
    echo "Error: Release v${VERSION} has no macOS ${ARCH_LABEL} disk image." >&2
    exit 1
  fi
  DMG_NAME="${DMG_URL##*/}"
  DMG_PATH="/tmp/${DMG_NAME}"

  echo "Downloading ${DMG_NAME}..."
  curl -fSL --progress-bar -o "$DMG_PATH" "$DMG_URL"

  if [ -d "${INSTALL_DIR}/${APP_NAME}.app" ]; then
    printf "%s.app already exists in %s. Overwrite? [y/N] " "$APP_NAME" "$INSTALL_DIR"
    read -r answer
    case "$answer" in
      [yY]|[yY][eE][sS])
        rm -rf "${INSTALL_DIR}/${APP_NAME}.app"
        ;;
      *)
        echo "Aborted."
        rm -f "$DMG_PATH"
        exit 0
        ;;
    esac
  fi

  echo "Installing to ${INSTALL_DIR}..."
  MOUNT_DIR="$(hdiutil attach -nobrowse -noautoopen "$DMG_PATH" | tail -1 | awk -F'\t' '{print $NF}')"
  cp -R "${MOUNT_DIR}/${APP_NAME}.app" "${INSTALL_DIR}/"
  hdiutil detach "$MOUNT_DIR" -quiet
  rm -f "$DMG_PATH"

  echo ""
  echo "${APP_NAME} v${VERSION} installed to ${INSTALL_DIR}/${APP_NAME}.app"
  echo "Run it from your Applications folder or with: open -a ${APP_NAME}"
}

main
