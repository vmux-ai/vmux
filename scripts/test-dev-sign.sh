#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/client/scripts" "$tmp/client/packaging/macos" "$tmp/bin" "$tmp/output"
cp "$root/scripts/sign-dev-mac.sh" "$tmp/client/scripts/"
cp "$root/scripts/product-host.sh" "$tmp/client/scripts/"
cp "$root/packaging/macos/VmuxDev.entitlements" "$tmp/client/packaging/macos/"
printf 'compiled desktop\n' > "$tmp/output/desktop"
printf 'linker-signed helper\n' > "$tmp/output/helper"
cat > "$tmp/bin/file" <<'FILE'
#!/usr/bin/env bash
printf '%s: Mach-O executable arm64\n' "$1"
FILE
cat > "$tmp/bin/codesign" <<'SIGN'
#!/usr/bin/env bash
set -euo pipefail
if [[ " $* " == *' --force '* ]]; then
    binary="${!#}"
    printf '%s\n' "$binary" >> "$TEST_SIGN_LOG"
    printf 'signed with %s\n' "$APPLE_SIGNING_IDENTITY" >> "$binary"
fi
SIGN
chmod +x "$tmp/bin/"* "$tmp/client/scripts/sign-dev-mac.sh"
export TEST_PRODUCT_BUILDER="$("$root/scripts/vmux-builder" --print-bin)"
export TEST_PRODUCT_ROOT="$root"
export VMUX_HOST_DEV_ENTITLEMENTS="$tmp/client/packaging/macos/VmuxDev.entitlements"
cat > "$tmp/client/scripts/vmux-builder" <<'PRODUCT'
#!/usr/bin/env bash
exec "$TEST_PRODUCT_BUILDER" --root "$TEST_PRODUCT_ROOT" host-config
PRODUCT
chmod +x "$tmp/client/scripts/vmux-builder"
export PATH="$tmp/bin:$PATH"
export APP_BINARY="$tmp/output/desktop"
export HELPER_BINARY="$tmp/output/helper"
export APPLE_SIGNING_IDENTITY='fixture identity'
export CODESIGN_KEYCHAIN='fixture keychain'
export TEST_SIGN_LOG="$tmp/sign.log"

"$tmp/client/scripts/sign-dev-mac.sh"
[[ "$(wc -l < "$TEST_SIGN_LOG" | tr -d ' ')" == 2 ]]
grep -Fxq "$HELPER_BINARY" "$TEST_SIGN_LOG"
helper_before="$(shasum -a 256 "$HELPER_BINARY")"
"$tmp/client/scripts/sign-dev-mac.sh"
[[ "$(wc -l < "$TEST_SIGN_LOG" | tr -d ' ')" == 2 ]]
[[ "$helper_before" == "$(shasum -a 256 "$HELPER_BINARY")" ]]
printf 'rebuilt with a valid linker signature\n' >> "$HELPER_BINARY"
"$tmp/client/scripts/sign-dev-mac.sh"
[[ "$(wc -l < "$TEST_SIGN_LOG" | tr -d ' ')" == 3 ]]
[[ "$(tail -n 1 "$TEST_SIGN_LOG")" == "$HELPER_BINARY" ]]
printf '\n' >> "$tmp/client/packaging/macos/VmuxDev.entitlements"
"$tmp/client/scripts/sign-dev-mac.sh"
[[ "$(wc -l < "$TEST_SIGN_LOG" | tr -d ' ')" == 5 ]]
APPLE_SIGNING_IDENTITY='replacement identity' "$tmp/client/scripts/sign-dev-mac.sh"
[[ "$(wc -l < "$TEST_SIGN_LOG" | tr -d ' ')" == 7 ]]
printf 'Debug helper signing, cache reuse, and configuration changes passed\n'
