#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-update}"

if [[ "$MODE" != "update" && "$MODE" != "--check" ]]; then
    echo "usage: $0 [--check]" >&2
    exit 2
fi

for command in gh jq sed cmp mktemp; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "missing required command: $command" >&2
        exit 1
    fi
done

VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)"
if [[ -z "$VERSION" ]]; then
    echo "workspace version not found" >&2
    exit 1
fi

TAG="v$VERSION"
DMG_NAME="Vmux_${VERSION}_aarch64.dmg"
BINARY_NAME="vmux-v${VERSION}-aarch64-apple-darwin.tar.gz"
APP_NAME="Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz"
SIG_NAME="${APP_NAME}.sig"
DMG_URL="https://github.com/vmux-ai/vmux/releases/download/${TAG}/${DMG_NAME}"
APP_URL="https://github.com/vmux-ai/vmux/releases/download/${TAG}/${APP_NAME}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

RELEASE_JSON="$TMP_DIR/release.json"
if ! gh release view "$TAG" --json isDraft,assets > "$RELEASE_JSON"; then
    echo "unable to read release $TAG; wait for release PR CI or check GitHub access" >&2
    exit 1
fi

for asset in "$DMG_NAME" "$BINARY_NAME" "$APP_NAME" "$SIG_NAME"; do
    if ! jq -e --arg name "$asset" '.assets[] | select(.name == $name)' "$RELEASE_JSON" >/dev/null; then
        echo "release $TAG is missing asset: $asset" >&2
        exit 1
    fi
done

DMG_DIGEST="$(jq -er --arg name "$DMG_NAME" '.assets[] | select(.name == $name) | .digest' "$RELEASE_JSON")"
DMG_SHA="${DMG_DIGEST#sha256:}"
if [[ ! "$DMG_SHA" =~ ^[0-9a-f]{64}$ ]]; then
    echo "invalid DMG digest: $DMG_DIGEST" >&2
    exit 1
fi

MANIFEST="$ROOT/website/public/updates.json"
CURRENT_VERSION="$(jq -r '.version // empty' "$MANIFEST")"
CURRENT_PUB_DATE="$(jq -r '.pub_date // empty' "$MANIFEST")"
if [[ "$CURRENT_VERSION" == "$TAG" && -n "$CURRENT_PUB_DATE" ]]; then
    PUB_DATE="$CURRENT_PUB_DATE"
else
    PUB_DATE="$(jq -er --arg name "$APP_NAME" '.assets[] | select(.name == $name) | .createdAt' "$RELEASE_JSON")"
fi

ASSET_DIR="$TMP_DIR/assets"
mkdir "$ASSET_DIR"
gh release download "$TAG" --pattern "$SIG_NAME" --dir "$ASSET_DIR"
SIGNATURE="$(<"$ASSET_DIR/$SIG_NAME")"
if [[ -z "$SIGNATURE" ]]; then
    echo "empty update signature" >&2
    exit 1
fi

CASK="$ROOT/Casks/vmux.rb"
EXPECTED_CASK="$TMP_DIR/vmux.rb"
sed \
    -e "s/^  version \".*\"/  version \"${VERSION}\"/" \
    -e "s/^  sha256 \".*\"/  sha256 \"${DMG_SHA}\"/" \
    -e "s|^  url \".*\"|  url \"${DMG_URL}\"|" \
    "$CASK" > "$EXPECTED_CASK"

EXPECTED_MANIFEST="$TMP_DIR/updates.json"
jq \
    --arg version "$TAG" \
    --arg pub_date "$PUB_DATE" \
    --arg app_url "$APP_URL" \
    --arg signature "$SIGNATURE" \
    --arg dmg_url "$DMG_URL" \
    --arg dmg_name "$DMG_NAME" \
    '
      .version = $version
      | .pub_date = $pub_date
      | .platforms["macos-aarch64"].url = $app_url
      | .platforms["macos-aarch64"].signature = $signature
      | .platforms["macos-aarch64"].format = "app"
      | .downloads["macos-aarch64"].url = $dmg_url
      | .downloads["macos-aarch64"].filename = $dmg_name
    ' \
    "$MANIFEST" > "$EXPECTED_MANIFEST"

if [[ "$MODE" == "--check" ]]; then
    mismatch=0
    if ! cmp -s "$CASK" "$EXPECTED_CASK"; then
        echo "Casks/vmux.rb does not match release $TAG" >&2
        mismatch=1
    fi
    if ! cmp -s "$MANIFEST" "$EXPECTED_MANIFEST"; then
        echo "website/public/updates.json does not match release $TAG" >&2
        mismatch=1
    fi
    if [[ "$mismatch" -ne 0 ]]; then
        echo "run ./scripts/update-release-metadata.sh on the release PR branch, commit, and push" >&2
        exit 1
    fi
    echo "release metadata matches $TAG"
    exit 0
fi

cp "$EXPECTED_CASK" "$CASK"
cp "$EXPECTED_MANIFEST" "$MANIFEST"
echo "updated release metadata for $TAG"
