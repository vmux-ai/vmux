#!/bin/sh

vmux_release_artifacts() {
    if [ "$#" -ne 1 ] || [ -z "$1" ]; then
        echo "usage: vmux_release_artifacts <version>" >&2
        return 2
    fi

    VMUX_RELEASE_VERSION="$1"
    VMUX_RELEASE_REPOSITORY="${VMUX_RELEASE_REPOSITORY:-vmux-ai/vmux}"
    VMUX_RELEASE_TAG="v${VMUX_RELEASE_VERSION}"
    VMUX_DMG_NAME="Vmux_${VMUX_RELEASE_VERSION}_aarch64.dmg"
    VMUX_BINARY_ARCHIVE_NAME="vmux-v${VMUX_RELEASE_VERSION}-aarch64-apple-darwin.tar.gz"
    VMUX_APP_ARCHIVE_NAME="Vmux-v${VMUX_RELEASE_VERSION}-aarch64-apple-darwin.app.tar.gz"
    VMUX_APP_SIGNATURE_NAME="${VMUX_APP_ARCHIVE_NAME}.sig"
    VMUX_RELEASE_BASE_URL="https://github.com/${VMUX_RELEASE_REPOSITORY}/releases/download/${VMUX_RELEASE_TAG}"
    VMUX_DMG_URL="${VMUX_RELEASE_BASE_URL}/${VMUX_DMG_NAME}"
    VMUX_APP_URL="${VMUX_RELEASE_BASE_URL}/${VMUX_APP_ARCHIVE_NAME}"
}
