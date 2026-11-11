#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$root"
source "$root/scripts/cargo-target-paths.sh"
consumer_manifest="$root/examples/framework-consumer/Cargo.toml"
consumer_target="$(vmux_cargo_target_dir "$root")/framework-consumer"

cargo test --locked -p vmux_ecs --no-default-features --lib
cargo test --locked -p vmux_ecs --no-default-features --features persistence --lib
cargo test --locked --manifest-path "$consumer_manifest" --target-dir "$consumer_target"

python3 - "$consumer_manifest" <<'PY'
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
required = {"vmux_app", "vmux_ecs"}
if not required.issubset(packages):
    raise SystemExit("The consumer must exercise the public application and ECS packages")
forbidden = {"bevy_cef", "bevy_cef_core", "bevy_winit", "vmux_profile", "vmux_ui", "vmux_layout", "wry"}
linked = sorted(packages.intersection(forbidden))
if linked:
    raise SystemExit(f"Framework consumer links native application dependencies: {linked}")
print("Framework consumer uses the public core without Profile or UI dependencies")
PY
