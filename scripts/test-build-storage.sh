#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
tmp="$(mktemp -d)"
tmp="$(cd "$tmp" && pwd -P)"
trap 'rm -rf "$tmp"' EXIT
fixture="$tmp/checkout with spaces"
target="$tmp/target with spaces"
mkdir -p "$fixture/scripts" "$target/debug/deps" "$target/release" "$tmp/bin"
cp "$root/Makefile" "$root/tool-versions.env" "$root/Cargo.lock" "$fixture/"
cp "$root/scripts/with-build-storage.sh" "$root/scripts/build-storage.py" \
    "$root/scripts/target-cache-common.sh" "$root/scripts/cargo-target-paths.sh" \
    "$root/scripts/build-mac-release.sh" "$fixture/scripts/"
printf 'Signature: 8a477f597d28d172789f06886806bc55\n' > "$target/CACHEDIR.TAG"
cat > "$tmp/bin/ps" <<'PS'
#!/usr/bin/env bash
printf '%s fixture\n' "$$"
PS
cat > "$tmp/bin/lsof" <<'LSOF'
#!/usr/bin/env bash
printf 'p%s\nn/\n' "$$"
LSOF
cat > "$tmp/bin/security" <<'SECURITY'
#!/usr/bin/env bash
exit 0
SECURITY
cat > "$tmp/bin/codesign" <<'SIGN'
#!/usr/bin/env bash
[[ "${FAIL_SIGNATURE:-}" != 1 ]] || exit 43
if [[ "$1" == --display ]]; then
    printf 'certificate\n' > codesign0
fi
SIGN
cat > "$tmp/bin/git" <<'GIT'
#!/usr/bin/env bash
if [[ "$*" == *'rev-parse --short=7 HEAD'* ]]; then
    printf '9999999\n'
else
    exec /usr/bin/git "$@"
fi
GIT
cat > "$tmp/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
if [[ "${1:-}" == metadata ]]; then
    printf '{"packages":[]}\n'
else
    echo "Unexpected real compilation in storage fixture: $*" >&2
    exit 1
fi
CARGO
cat > "$fixture/scripts/package.sh" <<'PACKAGE'
#!/usr/bin/env bash
set -euo pipefail
printf 'package:%s\n' "$1" >> "$TEST_STORAGE_LOG"
[[ "${FAIL_PACKAGE:-}" != 1 ]] || exit 42
if [[ "$1" == local ]]; then
    app="$CARGO_TARGET_DIR/release/Vmux (9999999).app"
else
    app="$CARGO_TARGET_DIR/release/Vmux.app"
    printf 'distribution image\n' > "$CARGO_TARGET_DIR/release/Vmux.dmg"
fi
mkdir -p "$app/Contents/MacOS"
printf 'signed executable\n' > "$app/Contents/MacOS/vmux_desktop"
mkdir -p "$CARGO_TARGET_DIR/release/deps"
printf 'compile artifact\n' > "$CARGO_TARGET_DIR/release/deps/cache.o"
PACKAGE
cat > "$fixture/scripts/fixture-build.sh" <<'BUILD'
#!/usr/bin/env bash
set -euo pipefail
[[ "$VMUX_TARGET_LOCK_TARGET" == "$CARGO_TARGET_DIR" ]]
[[ "$VMUX_BUILD_STORAGE_TARGET" == "$CARGO_TARGET_DIR" ]]
kill -0 "$VMUX_TARGET_LOCK_OWNER_PID"
printf 'build:%s\n' "$1" >> "$TEST_STORAGE_LOG"
if [[ "$1" == dev ]]; then
    mkdir -p "$CARGO_TARGET_DIR/debug/Vmux Dev.app/Contents/MacOS"
    printf 'prepared executable\n' > "$CARGO_TARGET_DIR/debug/Vmux Dev.app/Contents/MacOS/vmux_desktop"
    printf 'compile artifact\n' > "$CARGO_TARGET_DIR/debug/deps/cache.o"
    printf 'runtime library\n' > "$CARGO_TARGET_DIR/debug/deps/current.dylib"
else
    exec ./scripts/build-mac-release.sh "$1"
fi
BUILD
cat >> "$fixture/Makefile" <<'MAKE'

ensure-native-deps ensure-codesign-deps install-debug-render-process ensure-package-deps:
	@printf 'prerequisite\n' >> "$$TEST_STORAGE_LOG"
_build-dev:
	@./scripts/fixture-build.sh dev
_build-local:
	@./scripts/fixture-build.sh local
_build-release:
	@./scripts/fixture-build.sh release
MAKE
chmod +x "$tmp/bin/"* "$fixture/scripts/"*.sh
export PATH="$tmp/bin:$PATH"
export CARGO_TARGET_DIR="$target"
export APPLE_SIGNING_IDENTITY='fixture identity'
export TEST_STORAGE_LOG="$tmp/build.log"
export VMUX_BUILD_STORAGE_MIN_GIB=0
export VMUX_BUILD_STORAGE_WARN_GIB=0
export VMUX_BUILD_STORAGE_CACHE_GIB=0
printf 'CARGO_TARGET_DIR=%s\n' "$target" > "$fixture/.env"

for number in 1 2 3 4 5; do
    app="$target/release/Vmux (000000$number).app"
    mkdir -p "$app/Contents/MacOS"
    printf 'old signed executable\n' > "$app/Contents/MacOS/vmux_desktop"
done
if FAIL_PACKAGE=1 make -C "$fixture" build-local; then
    echo "A failed package was accepted" >&2
    exit 1
fi
for number in 1 2 3 4 5; do
    [[ -f "$target/release/Vmux (000000$number).app/Contents/MacOS/vmux_desktop" ]]
done
if FAIL_SIGNATURE=1 make -C "$fixture" build-local; then
    echo "A failed signature verification was accepted" >&2
    exit 1
fi
for number in 1 2 3 4 5; do
    [[ -f "$target/release/Vmux (000000$number).app/Contents/MacOS/vmux_desktop" ]]
done
make -C "$fixture" build-dev
[[ -f "$target/debug/Vmux Dev.app/Contents/MacOS/vmux_desktop" ]]
[[ -f "$target/debug/deps/current.dylib" ]]
[[ ! -f "$target/debug/deps/cache.o" ]]
make -C "$fixture" build-local
[[ -f "$target/release/Vmux (9999999).app/Contents/MacOS/vmux_desktop" ]]
[[ ! -f "$target/release/deps/cache.o" ]]
python3 - "$target" <<'CHECK'
from pathlib import Path
import sys
target = Path(sys.argv[1])
assert len(list((target / 'release').glob('Vmux (*).app'))) == 3
CHECK
make -C "$fixture" build-release
[[ -f "$target/release/Vmux.app/Contents/MacOS/vmux_desktop" ]]
[[ "$(cat "$target/release/Vmux.dmg")" == 'distribution image' ]]
[[ ! -f "$target/release/deps/cache.o" ]]

for goal in build-dev build-local build-release; do
    before="$(wc -l < "$TEST_STORAGE_LOG")"
    if VMUX_BUILD_STORAGE_MIN_GIB=1000000 VMUX_BUILD_STORAGE_WARN_GIB=1000000 make -C "$fixture" "$goal"; then
        echo "A build started with insufficient disk space: $goal" >&2
        exit 1
    fi
    [[ "$(wc -l < "$TEST_STORAGE_LOG")" == "$before" ]]
done

printf 'retained cache\n' > "$target/debug/deps/cache.o"
make -C "$fixture" clean-build-cache
[[ -f "$target/debug/deps/cache.o" ]]
VMUX_BUILD_STORAGE_APPLY=1 make -C "$fixture" clean-build-cache
[[ ! -f "$target/debug/deps/cache.o" ]]
[[ -f "$target/debug/deps/current.dylib" ]]
printf 'Shared dev/local/release storage lifecycle passed\n'
