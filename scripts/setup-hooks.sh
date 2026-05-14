#!/usr/bin/env bash
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
GIT_DIR="$(git rev-parse --git-common-dir)"
mkdir -p "$GIT_DIR/hooks"
ln -sf "$ROOT/scripts/hooks/pre-push" "$GIT_DIR/hooks/pre-push"
echo "Installed pre-push hook -> scripts/hooks/pre-push"
