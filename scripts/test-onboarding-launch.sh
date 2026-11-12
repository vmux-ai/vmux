#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
python3 - "$root" <<'PY'
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

repository = Path(sys.argv[1])
with tempfile.TemporaryDirectory(prefix="vmux-fresh-launch-") as temporary:
    fixture = Path(temporary).resolve()
    root = fixture / "worktree"
    scripts = root / "scripts"
    scripts.mkdir(parents=True)
    for name in ("run-dev.sh", "run-onboarding-test.sh", "cargo-target-paths.sh"):
        shutil.copy2(repository / "scripts" / name, scripts / name)

    def executable(path, content):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        path.chmod(0o755)

    record = '''#!/usr/bin/env python3
import json, os, sys
values = {name: os.environ.get(name, "") for name in (
    "VMUX_PROFILE", "VMUX_DATA_DIR", "VMUX_TEST_ENV_KEYS", "VMUX_ONBOARDING_FRESH")}
for index, argument in enumerate(sys.argv[1:]):
    if argument == "--env":
        name, value = sys.argv[index + 2].split("=", 1)
        values[name] = value
print(json.dumps(values))
'''
    debug = root / "target" / "debug"
    app = debug / "Vmux Dev.app"
    binaries = [debug / name for name in ("vmux", "vmux_service", "vmux_desktop")]
    binaries.append(app / "Contents/MacOS/vmux_desktop")
    for binary in binaries:
        executable(binary, record)
    stamp = str(root) + "\n"
    for binary in binaries:
        stamp += hashlib.sha256(binary.read_bytes()).hexdigest() + "  " + str(binary) + "\n"
    (debug / ".vmux-dev-ready").write_text(stamp)
    executable(root / "bin/open", record)
    executable(root / "bin/uname", "#!/bin/sh\necho Darwin\n")
    executable(root / "bin/rustc", "#!/bin/sh\necho /fixture/rust-libraries\n")
    executable(root / "bin/codesign", "#!/bin/sh\nexit 0\n")

    data = fixture / "data"
    data.mkdir()
    protected = fixture / "persistent"
    protected.mkdir()
    (protected / "sentinel").write_text("personal state")
    environment = dict(os.environ)
    environment.pop("CARGO_BUILD_TARGET", None)
    environment.pop("VMUX_ONBOARDING_FRESH", None)
    environment.update(PATH=str(root / "bin") + os.pathsep + environment["PATH"],
                       TMPDIR=str(data), CARGO_TARGET_DIR=str(root / "target"),
                       VMUX_PROFILE="personal", VMUX_DATA_DIR=str(protected),
                       VMUX_TEST_ENV_KEYS="0")

    def run(script, updates=None, arguments=()):
        configured = dict(environment)
        configured.update(updates or {})
        result = subprocess.run([str(scripts / script), *arguments], env=configured,
                                capture_output=True, text=True, timeout=15)
        assert result.returncode == 0, (result.stdout, result.stderr)
        return json.loads(result.stdout.splitlines()[-1])

    fresh = {"VMUX_ONBOARDING_FRESH": "1", "VMUX_PROFILE": "fresh", "VMUX_TEST_ENV_KEYS": "1"}
    first = run("run-dev.sh", fresh)
    first_data = Path(first["VMUX_DATA_DIR"])
    assert first["VMUX_PROFILE"] == "fresh", first
    assert first["VMUX_TEST_ENV_KEYS"] == "0", first
    assert not first["VMUX_ONBOARDING_FRESH"], first
    assert first_data.parent == data, first
    assert (first_data / "profiles/fresh/home").is_dir()
    assert (first_data / "config/projects").is_dir()
    (first_data / "config/setup.ron").write_text("connected")

    second = run("run-dev.sh", fresh)
    second_data = Path(second["VMUX_DATA_DIR"])
    assert second_data != first_data, second
    assert not (second_data / "config/setup.ron").exists()
    assert (first_data / "config/setup.ron").read_text() == "connected"
    assert (protected / "sentinel").read_text() == "personal state"

    normal = run("run-dev.sh")
    assert normal["VMUX_PROFILE"] == "personal", normal
    assert normal["VMUX_DATA_DIR"] == str(protected), normal
    before_stop = set(data.iterdir())
    stopped = run("run-dev.sh", fresh, ("stop",))
    assert stopped["VMUX_DATA_DIR"] == str(protected), stopped
    assert set(data.iterdir()) == before_stop

    reused = run("run-onboarding-test.sh", fresh, ("clean", str(first_data)))
    assert reused["VMUX_DATA_DIR"] == str(first_data), reused
    assert (first_data / "config/setup.ron").read_text() == "connected"
    inherited = run("run-onboarding-test.sh", fresh, ("env",))
    assert inherited["VMUX_PROFILE"] == "fresh-env", inherited
    assert inherited["VMUX_TEST_ENV_KEYS"] == "1", inherited

print("Fresh launches, persistent profiles, stop, fixture reuse, and env-key isolation passed")
PY
