#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/client/scripts" "$tmp/target/debug/deps" "$tmp/bin"
cp "$root/scripts/build-dev.sh" "$root/scripts/run-dev.sh" "$root/scripts/cargo-target-paths.sh" "$tmp/client/scripts/"

cat > "$tmp/client/scripts/cargo-with-cef-cache.sh" <<'CARGO'
#!/usr/bin/env bash
set -eu
printf '%s|%s|%s\n' "$CARGO_INCREMENTAL" "$VMUX_BUILD_PROFILE" "$*" >> "$TEST_LOG"
[[ "${FAIL_BUILD:-}" != "1" ]] || exit 42
for binary in vmux vmux_service vmux_desktop; do
    printf '#!/usr/bin/env bash\nprintf "%%s|%%s|%%s|%%s\\n" "$VMUX_PROFILE" "${CEF_PATH-unset}" "$DYLD_LIBRARY_PATH" "$*" >> "$TEST_RUN"\n' > "$CARGO_TARGET_DIR/debug/$binary"
    chmod +x "$CARGO_TARGET_DIR/debug/$binary"
done
CARGO
cat > "$tmp/bin/uname" <<'UNAME'
#!/usr/bin/env bash
printf 'Linux\n'
UNAME
cat > "$tmp/bin/rustc" <<'RUSTC'
#!/usr/bin/env bash
printf '/test/toolchain/lib\n'
RUSTC
cat > "$tmp/bin/env" <<'ENV'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$TEST_ENV"
exec /usr/bin/env "$@"
ENV
chmod +x "$tmp/client/scripts/"*.sh "$tmp/bin/"*
export PATH="$tmp/bin:$PATH"
export CARGO_TARGET_DIR="$tmp/target"
export TEST_LOG="$tmp/build.log"
export TEST_RUN="$tmp/run.log"
export TEST_ENV="$tmp/env.log"

"$tmp/client/scripts/build-dev.sh" --no-default-features --features dev,full
[[ "$(wc -l < "$TEST_LOG" | tr -d ' ')" == 3 ]]
[[ "$(grep -c '^1|dev|' "$TEST_LOG")" == 3 ]]
VMUX_PROFILE=work CEF_PATH=wrong "$tmp/client/scripts/run-dev.sh"
[[ "$(wc -l < "$TEST_LOG" | tr -d ' ')" == 3 ]]
grep -Fq 'work|unset|' "$TEST_RUN"
grep -Fq -- '-u CEF_PATH DYLD_LIBRARY_PATH=/test/toolchain/lib:' "$TEST_ENV"
printf '\nchanged\n' >> "$tmp/target/debug/vmux_desktop"
if "$tmp/client/scripts/run-dev.sh"; then
    echo "run-dev accepted changed artifacts" >&2
    exit 1
fi
if FAIL_BUILD=1 "$tmp/client/scripts/build-dev.sh" --features dev,full; then
    echo "build-dev accepted a failed build" >&2
    exit 1
fi
[[ ! -f "$tmp/target/debug/.vmux-dev-ready" ]]
"$tmp/client/scripts/run-dev.sh" stop
grep -Fq 'service stop' "$TEST_RUN"
printf 'build-only and launch-only lifecycle passed\n'
