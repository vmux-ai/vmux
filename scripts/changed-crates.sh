#!/usr/bin/env bash
# Print the set of workspace crates that should be linted/tested,
# given changes vs BASE (default: origin/main).
#
# Includes:
#   1. Crates whose own files changed.
#   2. Crates whose source contains include_str!("…") referencing a changed file.
#
# Vendored `patches/` are excluded.

set -euo pipefail

BASE="${BASE:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

crates_by_dir() {
  cargo metadata --no-deps --format-version 1 \
    | jq -r '.packages[]
        | select(.manifest_path | test("patches") | not)
        | "\(.name)\t\(.manifest_path | sub("/Cargo\\.toml$"; ""))"' \
    | while IFS=$'\t' read -r name dir; do
        rel="${dir#"$ROOT"/}"
        [ -z "$rel" ] && rel="."
        if ! git diff --quiet "$BASE" -- "$rel"; then
          printf '%s\n' "$name"
        fi
      done
}

crates_by_includes() {
  changed="$(git diff --name-only "$BASE")"
  [ -z "$changed" ] && return 0
  cargo metadata --no-deps --format-version 1 \
    | jq -r '.packages[]
        | select(.manifest_path | test("patches") | not)
        | "\(.name)\t\(.manifest_path | sub("/Cargo\\.toml$"; ""))"' \
    | while IFS=$'\t' read -r name dir; do
        { grep -rn 'include_str!("[^"]*")' "$dir" 2>/dev/null || true; } \
          | while IFS=':' read -r file _lineno match; do
              [ -z "$file" ] && continue
              inc="$(printf '%s' "$match" | sed -E 's/.*include_str!\("([^"]*)"\).*/\1/')"
              file_dir="$(dirname "$file")"
              abs="$(cd "$file_dir" && python3 -c "import os,sys; print(os.path.normpath(os.path.join(os.getcwd(), sys.argv[1])))" "$inc" 2>/dev/null)" || continue
              rel="${abs#"$ROOT"/}"
              if printf '%s\n' "$changed" | grep -qx "$rel"; then
                printf '%s\n' "$name"
              fi
            done
      done
}

(crates_by_dir; crates_by_includes) | sort -u
