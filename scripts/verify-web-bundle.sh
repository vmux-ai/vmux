#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist="${1:-${VMUX_WEB_BUNDLE_DIST:-$ROOT/crates/util/vmux_ui/dist}}"
stamp="$dist/.bundle-stamp"

if [[ ! -f "$stamp" ]]; then
  echo "stylesheet bundle is incomplete: no $stamp" >&2
  exit 1
fi

stamp_paths="$(mktemp)"
actual_paths="$(mktemp)"
trap 'rm -f "$stamp_paths" "$actual_paths"' EXIT

while IFS= read -r line; do
  hash="${line%%  *}"
  path="${line#*  }"
  if [[ "$line" == "$path" || "${#hash}" -ne 64 || "$hash" == *[!0-9a-f]* ]]; then
    echo "stylesheet bundle stamp has an invalid entry" >&2
    exit 1
  fi
  case "$path" in
    ''|/*|.|..|./*|*/./*|*/.|../*|*/../*|*/..|*//*)
      echo "stylesheet bundle stamp has an unsafe path" >&2
      exit 1
      ;;
  esac
  printf '%s\n' "$path" >> "$stamp_paths"
done < "$stamp"

LC_ALL=C sort -o "$stamp_paths" "$stamp_paths"
(
  cd "$dist"
  find . -type f ! -path './.bundle-stamp' -print | sed 's#^\./##' | LC_ALL=C sort
) > "$actual_paths"

if ! cmp -s "$stamp_paths" "$actual_paths"; then
  echo "stylesheet bundle stamp paths do not match bundle files" >&2
  exit 1
fi

if command -v shasum >/dev/null 2>&1; then
  (cd "$dist" && shasum -a 256 --check .bundle-stamp >/dev/null)
elif command -v sha256sum >/dev/null 2>&1; then
  (cd "$dist" && sha256sum --check .bundle-stamp >/dev/null)
else
  echo "SHA-256 tool not found" >&2
  exit 1
fi
