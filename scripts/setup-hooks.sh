#!/usr/bin/env bash
set -euo pipefail
GIT_DIR="$(cd "$(git rev-parse --git-common-dir)" && pwd)"
ROOT="$(dirname "$GIT_DIR")"
mkdir -p "$GIT_DIR/hooks"
ln -sf "$ROOT/scripts/hooks/pre-push" "$GIT_DIR/hooks/pre-push"
echo "Installed pre-push hook -> $ROOT/scripts/hooks/pre-push"
