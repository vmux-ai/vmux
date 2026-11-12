#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$root"
source "$root/scripts/cargo-target-paths.sh"
consumer_manifest="$root/examples/framework-consumer/Cargo.toml"
consumer_target="$(vmux_cargo_target_dir "$root")/framework-consumer"
consumer_revision=""
git_workspace=""

if [[ "$#" -gt 1 ]]; then
    echo "usage: $0 [--git]" >&2
    exit 2
fi

case "${1:-}" in
    "") ;;
    --git)
        git_workspace="$(mktemp -d "${TMPDIR:-/tmp}/vmux-git-consumer.XXXXXX")"
        trap 'rm -rf -- "$git_workspace"' EXIT
        consumer_manifest="$git_workspace/Cargo.toml"
        consumer_target="$(vmux_cargo_target_dir "$root")/framework-git-consumer"
        consumer_revision="$(git rev-parse --verify "${VMUX_SDK_REVISION:-HEAD}^{commit}")"
        consumer_repository="${VMUX_SDK_REPOSITORY:-file://$(git rev-parse --path-format=absolute --git-common-dir)}"
        cp -R "$root/examples/framework-consumer/src" "$root/examples/framework-consumer/tests" "$git_workspace/"
        cp "$root/examples/framework-consumer/Cargo.lock" "$git_workspace/"
        python3 - "$root/examples/framework-consumer/Cargo.toml" "$consumer_manifest" "$consumer_repository" "$consumer_revision" <<'PY'
import json
from pathlib import Path
import re
import sys
import tomllib

source, destination, repository, revision = sys.argv[1:]
lines = []
for line in Path(source).read_text().splitlines():
    if re.match(r"^vmux_\w+\s*=", line):
        line = re.sub(r'path\s*=\s*"[^"]+"', f"git = {json.dumps(repository)}, rev = {json.dumps(revision)}", line)
    lines.append(line)
manifest = "\n".join(lines) + "\n"
dependencies = tomllib.loads(manifest)["dependencies"]
for name, dependency in dependencies.items():
    if name.startswith("vmux_") and (not isinstance(dependency, dict) or dependency.get("git") != repository or dependency.get("rev") != revision):
        raise SystemExit(f"SDK dependency {name} must use the same Git revision")
Path(destination).write_text(manifest)
PY
        cargo metadata --format-version 1 --manifest-path "$consumer_manifest" >/dev/null
        ;;
    *)
        echo "usage: $0 [--git]" >&2
        exit 2
        ;;
esac

if [[ -z "$consumer_revision" ]]; then
    cargo test --locked -p vmux_ecs --no-default-features --lib
    cargo test --locked -p vmux_ecs --no-default-features --features contracts --lib
    cargo test --locked -p vmux_ecs --no-default-features --features store --lib
    cargo test --locked -p vmux_ecs --no-default-features --features jobs --lib
    cargo test --locked -p vmux_project --no-default-features --features contracts,filesystem --lib
    cargo test --locked -p vmux_extension --no-default-features --features filesystem --lib
    cargo test --locked -p vmux_flex --no-default-features
    cargo test --locked -p vmux_flex --no-default-features --features window --lib
    cargo test --locked -p vmux_pane --no-default-features --lib
    cargo test --locked -p vmux_pane --no-default-features --features store --lib
fi
cargo test --locked --manifest-path "$consumer_manifest" --target-dir "$consumer_target"

python3 - "$consumer_manifest" "$consumer_revision" <<'PY'
import json
import subprocess
import sys

result = subprocess.run(
    ["cargo", "metadata", "--locked", "--format-version", "1", "--manifest-path", sys.argv[1]],
    check=True,
    capture_output=True,
    text=True,
)
metadata = json.loads(result.stdout)
packages = {package["name"] for package in metadata["packages"]}
required = {"vmux_app", "vmux_ecs", "vmux_project", "vmux_flex", "vmux_pane", "vmux_extension"}
if not required.issubset(packages):
    raise SystemExit("The consumer must exercise the public application and ECS packages")
forbidden = {"bevy", "bevy_cef", "bevy_cef_core", "bevy_winit", "vmux_profile", "vmux_editor", "vmux_ui", "vmux_layout", "wry"}
linked = sorted(packages.intersection(forbidden))
if linked:
    raise SystemExit(f"Framework consumer links native application dependencies: {linked}")
if sys.argv[2]:
    for package in metadata["packages"]:
        if package["name"].startswith("vmux_"):
            source = package.get("source") or ""
            if not source.startswith("git+") or not source.endswith("#" + sys.argv[2]):
                raise SystemExit(f"SDK package {package['name']} is not pinned to the tested Git revision")
    print("All SDK packages use the same immutable Git revision")
print("Framework consumer uses the public core without Profile or UI dependencies")
PY
