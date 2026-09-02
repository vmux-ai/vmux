#!/usr/bin/env bash
set -euo pipefail
HOOKS="$(cd "$(git rev-parse --git-common-dir)" && pwd)/hooks"
mkdir -p "$HOOKS"
cat > "$HOOKS/pre-push" <<'HOOK'
#!/usr/bin/env bash
set -euo pipefail
exec "$(git rev-parse --show-toplevel)/scripts/hooks/pre-push" "$@"
HOOK
chmod +x "$HOOKS/pre-push"
echo "Installed pre-push hook -> $HOOKS/pre-push"
