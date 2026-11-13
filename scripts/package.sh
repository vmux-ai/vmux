#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${1:-local}"
export PATH="${HOME}/.cargo/bin:${PATH}"
source "$ROOT/scripts/cargo-target-paths.sh"
source "$ROOT/scripts/product-host.sh"
vmux_host_read "$ROOT"

CARGO_TOML="$ROOT/crates/app/vmux_desktop/Cargo.toml"
INFO_PLIST="$ROOT/packaging/macos/Info.plist"

case "$PROFILE" in
    release)
        PRODUCT_NAME="$VMUX_HOST_NAME"
        BUNDLE_ID="$VMUX_HOST_BUNDLE_ID"
        ;;
    local)
        SHA="$(git -C "$ROOT" rev-parse --short=7 HEAD)"
        PRODUCT_NAME="$VMUX_HOST_NAME ($SHA)"
        BUNDLE_ID="$VMUX_HOST_BUNDLE_ID.$SHA"
        ;;
    *)
        echo "Unknown profile: $PROFILE (expected: release, local)" >&2
        exit 1
        ;;
esac

if [[ "${VMUX_CI_FAST_PACKAGE:-}" == "1" ]]; then
    fast_event=0
    if [[ "${GITHUB_EVENT_NAME:-}" == "pull_request" ]]; then
        fast_event=1
    elif [[ "${GITHUB_EVENT_NAME:-}" == "push" && "${VMUX_CI_PREFLIGHT:-}" == "true" ]]; then
        case "${GITHUB_REF:-}" in
            refs/heads/main|refs/heads/master|refs/heads/release/*) ;;
            refs/heads/*) fast_event=1 ;;
        esac
    fi
    if [[ "$PROFILE" != "release" || "$fast_event" != "1" || "${IS_RELEASE_PR:-}" != "0" ]]; then
        echo "Fast package mode requires a non-release pull request or feature-branch preflight" >&2
        exit 1
    fi
    export SKIP_NOTARIZE=1
fi

echo "==> Packaging profile: $PROFILE"
echo "    Product name: $PRODUCT_NAME"
echo "    Bundle ID:    $BUNDLE_ID"

package_backup="$(mktemp -d)"
cp "$CARGO_TOML" "$package_backup/Cargo.toml"
cp "$INFO_PLIST" "$package_backup/Info.plist"

restore() {
    cp "$package_backup/Cargo.toml" "$CARGO_TOML"
    cp "$package_backup/Info.plist" "$INFO_PLIST"
    rm -rf "$package_backup"
    return 0
}
trap restore EXIT

python3 - "$CARGO_TOML" "$INFO_PLIST" "$PRODUCT_NAME" "$BUNDLE_ID" <<'PY'
import json, os, pathlib, plistlib, re, sys
cargo_path, plist_path = map(pathlib.Path, sys.argv[1:3])
product_name, bundle_id = sys.argv[3:5]
metadata = json.loads(os.environ['VMUX_HOST_JSON'])
text = cargo_path.read_text()
values = {
    'product-name': json.dumps(product_name, ensure_ascii=False),
    'identifier': json.dumps(bundle_id),
    'icons': json.dumps([metadata['VMUX_HOST_ICON']], ensure_ascii=False),
    'entitlements': json.dumps(metadata['VMUX_HOST_ENTITLEMENTS'], ensure_ascii=False),
    'info-plist-path': json.dumps(metadata['VMUX_HOST_USAGE_DESCRIPTIONS'], ensure_ascii=False),
    'background': json.dumps(metadata['VMUX_HOST_DMG_BACKGROUND'], ensure_ascii=False),
}
for key, value in values.items():
    text, count = re.subn(r'^'+re.escape(key)+r' = .*$', lambda _: key+' = '+value, text, flags=re.M)
    if count != 1:
        raise ValueError('Missing or ambiguous packager field: '+key)
text, count = re.subn(r'\{ path = "Vmux Service" \}', lambda _: '{ path = '+json.dumps(metadata['VMUX_HOST_SERVICE_NAME'], ensure_ascii=False)+' }', text)
if count != 1:
    raise ValueError('Missing or ambiguous service binary')
with plist_path.open('rb') as source:
    plist = plistlib.load(source)
plist.update(CFBundleIdentifier=bundle_id, CFBundleDisplayName=product_name, CFBundleName=product_name)
cargo_path.write_text(text)
with plist_path.open('wb') as destination:
    plistlib.dump(plist, destination)
PY

export VMUX_BUNDLE_ID="$BUNDLE_ID"
export VMUX_BUILD_PROFILE="$PROFILE"

APP_NAME="$PRODUCT_NAME"
export VMUX_CARGO_RELEASE_DIR="$(vmux_cargo_profile_dir "$ROOT" release)"
export VMUX_APP_BUNDLE="$VMUX_CARGO_RELEASE_DIR/$APP_NAME.app"

echo "==> Running cargo packager"
cd "$ROOT"
packager_args=(packager --release)
if [[ -n "${CARGO_BUILD_TARGET:-}" ]]; then
    packager_args+=(--target "$CARGO_BUILD_TARGET")
fi
if [[ "$PROFILE" == "local" || "${VMUX_CI_FAST_PACKAGE:-}" == "1" ]]; then
    packager_args+=(--formats app)
fi
VMUX_BUILD_PROFILE="$PROFILE" "$ROOT/scripts/cargo-with-cef-cache.sh" "${packager_args[@]}"

if [[ "${VMUX_CI_FAST_PACKAGE:-}" == "1" && ! -d "$VMUX_APP_BUNDLE" ]]; then
    echo "Fast package did not produce $VMUX_APP_BUNDLE" >&2
    exit 1
fi

if [[ ( "$PROFILE" == "local" || "${VMUX_CI_FAST_PACKAGE:-}" == "1" ) && -d "$VMUX_APP_BUNDLE" ]]; then
    if [[ "$PROFILE" == "local" ]]; then
        "$ROOT/scripts/build-title.sh" local "$ROOT" "$VMUX_APP_BUNDLE/Contents/Info.plist" >/dev/null
    fi
    echo "==> Injecting CEF into .app"
    CARGO_PACKAGER_FORMAT=dmg bash "$ROOT/scripts/inject-cef.sh"

    echo "==> Embedding launchd plist"
    VMUX_GIT_HASH="${SHA:-}" "$ROOT/scripts/embed-launch-agent-plist.sh"

    echo "==> Signing app bundle"
    APP_BUNDLE="$VMUX_APP_BUNDLE" VMUX_GIT_HASH="${SHA:-}" "$ROOT/scripts/sign-and-notarize.sh"
fi

echo "==> Packaging complete: $VMUX_APP_BUNDLE"
