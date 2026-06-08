#!/usr/bin/env bash
set -euo pipefail

# Inject CEF framework into .app bundle via bevy_cef_bundle_app.
# Called by cargo-packager as before-each-package-command.
# Only runs when processing DMG format (app already built by then).

if [[ "${CARGO_PACKAGER_FORMAT:-}" != "dmg" ]]; then
    echo "inject-cef: skipping (format=${CARGO_PACKAGER_FORMAT:-unknown}, waiting for dmg)"
    exit 0
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="${HOME}/.cargo/bin:${PATH}"
# Wrong CEF_PATH breaks cef-dll-sys; default macOS layout is ~/.local/share.
unset CEF_PATH

APP_BUNDLE="${VMUX_APP_BUNDLE:-${ROOT}/target/release/Vmux.app}"
BUNDLE_ID_BASE="${VMUX_BUNDLE_ID:-ai.vmux.desktop}"
CEF_FRAMEWORK="${CEF_FRAMEWORK:-${HOME}/.local/share/Chromium Embedded Framework.framework}"
HELPER_BIN="${ROOT}/target/release/bevy_cef_debug_render_process"

if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "inject-cef: .app not found at $APP_BUNDLE, skipping"
    exit 0
fi

APP_BUNDLE="$APP_BUNDLE" "$ROOT/scripts/copy-webview-assets.sh"

if [[ -d "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework" ]]; then
    echo "inject-cef: CEF already injected, skipping"
    exit 0
fi

if ! command -v bevy_cef_bundle_app >/dev/null 2>&1; then
    echo "inject-cef: bevy_cef_bundle_app not found. Install with: cargo install bevy_cef_bundle_app" >&2
    exit 1
fi

if [[ ! -f "$HELPER_BIN" ]]; then
    echo "inject-cef: helper binary not found at $HELPER_BIN" >&2
    echo "  Build it first: cargo build -p bevy_cef_debug_render_process --release" >&2
    exit 1
fi

echo "==> inject-cef: running bevy_cef_bundle_app"
bevy_cef_bundle_app --app "$APP_BUNDLE" --bundle-id-base "$BUNDLE_ID_BASE" --bin-name Vmux --cef-framework "$CEF_FRAMEWORK" --helper-bin "$HELPER_BIN" --no-sign

# Trim non-English Chromium locale packs to cut bundle size. Must run before
# the app is (re)signed/notarized below, since editing the framework
# invalidates its upstream signature.
CEF_RESOURCES="$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework/Resources"
if [[ -d "$CEF_RESOURCES" ]]; then
    keep_locales=("en.lproj" "en_GB.lproj" "Base.lproj")
    removed=0
    while IFS= read -r -d '' lproj; do
        base="$(basename "$lproj")"
        keep=0
        for k in "${keep_locales[@]}"; do
            [[ "$base" == "$k" ]] && keep=1 && break
        done
        if [[ "$keep" -eq 0 ]]; then
            rm -rf "$lproj"
            removed=$((removed + 1))
        fi
    done < <(find "$CEF_RESOURCES" -maxdepth 1 -name "*.lproj" -print0)
    echo "==> inject-cef: trimmed $removed non-English locale packs"
    if [[ ! -d "$CEF_RESOURCES/en.lproj" ]]; then
        echo "inject-cef: ERROR: en.lproj missing after locale trim (keep-list stale?)" >&2
        exit 1
    fi
fi

# Copy app icon (cargo-packager handles this via icons config, but ensure it's there)
ICNS_SRC="$ROOT/packaging/macos/Vmux.icns"
if [[ -f "$ICNS_SRC" && ! -f "$APP_BUNDLE/Contents/Resources/Vmux.icns" ]]; then
    mkdir -p "$APP_BUNDLE/Contents/Resources"
    cp -f "$ICNS_SRC" "$APP_BUNDLE/Contents/Resources/Vmux.icns"
fi

if [[ -f "$ICNS_SRC" ]]; then
    CEF_HELPER_APPS=(
        "Vmux Helper.app"
        "Vmux Helper (GPU).app"
        "Vmux Helper (Renderer).app"
        "Vmux Helper (Plugin).app"
        "Vmux Helper (Alerts).app"
    )
    for helper_name in "${CEF_HELPER_APPS[@]}"; do
        helper_app="$APP_BUNDLE/Contents/Frameworks/$helper_name"
        [[ -d "$helper_app" ]] || continue
        mkdir -p "$helper_app/Contents/Resources"
        cp -f "$ICNS_SRC" "$helper_app/Contents/Resources/Vmux.icns"
        plist="$helper_app/Contents/Info.plist"
        [[ -f "$plist" ]] || continue
        /usr/libexec/PlistBuddy -c "Set :CFBundleIconFile Vmux" "$plist" 2>/dev/null \
            || /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string Vmux" "$plist"
    done
fi

echo "==> inject-cef: done"
