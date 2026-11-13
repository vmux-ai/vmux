#![cfg(unix)]

use std::fs;

use support::Project;

pub mod support;

#[test]
fn prepared_debug_build_launches_without_recompiling_and_refuses_changed_artifacts() {
    let project = Project::create();
    fs::copy(
        project.repo().join("scripts/run-dev.sh"),
        project.root().join("scripts/run-dev.sh"),
    )
    .unwrap();
    project.script(
        "scripts/cargo-with-cef-cache.sh",
        r#"#!/bin/bash
set -eu
printf '%s|%s|%s\n' "$CARGO_INCREMENTAL" "$VMUX_BUILD_PROFILE" "$*" >> "$TEST_BUILD_LOG"
[[ "${FAIL_BUILD:-0}" != 1 ]] || exit 42
mkdir -p "$CARGO_TARGET_DIR/debug"
for binary in vmux vmux_service vmux_desktop; do
  printf '#!/bin/sh\nexit 0\n' > "$CARGO_TARGET_DIR/debug/$binary"
  chmod +x "$CARGO_TARGET_DIR/debug/$binary"
done
case " $* " in
  *' -p vmux_vm '*)
    printf '#!/bin/sh\nexit 0\n' > "$CARGO_TARGET_DIR/debug/vmux_vm"
    chmod +x "$CARGO_TARGET_DIR/debug/vmux_vm"
    ;;
esac
"#,
    );
    project.script(
        "scripts/ensure-local-codesign-identity.sh",
        "#!/bin/sh\nprintf 'fixture\\n'\n",
    );
    project.script("scripts/sign-dev-mac.sh", "#!/bin/sh\nexit 0\n");
    project.script(
        "scripts/sign-container-engine.sh",
        "#!/bin/sh\ntest -x \"$CARGO_TARGET_DIR/debug/vmux_vm\"\n",
    );
    project.script("scripts/bundle-dev.sh", r#"#!/bin/sh
mkdir -p "$CARGO_TARGET_DIR/debug/Vmux Dev.app/Contents/MacOS"
cp "$CARGO_TARGET_DIR/debug/vmux_desktop" "$CARGO_TARGET_DIR/debug/Vmux Dev.app/Contents/MacOS/vmux_desktop"
"#);
    project.script(
        "bin/open",
        r#"#!/bin/sh
for argument do app="$argument"; done
test -f "$app/Contents/MacOS/vmux_desktop"
"#,
    );
    let mut prepare = project.command(env!("CARGO_BIN_EXE_vmux_builder"));
    prepare.env("CARGO_INCREMENTAL", "1");
    prepare.arg("--root").arg(project.root()).args([
        "prepare-dev",
        "--no-default-features",
        "--features",
        "dev,full",
    ]);
    let output = prepare.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(project.root().join("build.log")).unwrap();
    assert_eq!(log.lines().count(), 3);
    assert!(log.lines().all(|line| line.starts_with("1|dev|")));
    let launch = project.command("./scripts/run-dev.sh").output().unwrap();
    assert!(
        launch.status.success(),
        "{}",
        String::from_utf8_lossy(&launch.stderr)
    );
    assert_eq!(
        fs::read_to_string(project.root().join("build.log")).unwrap(),
        log
    );
    let engine = project.target().join("debug/vmux_vm");
    fs::write(&engine, "#!/bin/sh\nexit 1\n").unwrap();
    assert!(
        !project
            .command("./scripts/run-dev.sh")
            .status()
            .unwrap()
            .success()
    );
    fs::write(engine, "#!/bin/sh\nexit 0\n").unwrap();
    fs::write(
        project.target().join("debug/vmux_desktop"),
        "#!/bin/sh\nexit 1\n",
    )
    .unwrap();
    assert!(
        !project
            .command("./scripts/run-dev.sh")
            .status()
            .unwrap()
            .success()
    );
    let failed = project
        .command(env!("CARGO_BIN_EXE_vmux_builder"))
        .arg("--root")
        .arg(project.root())
        .arg("prepare-dev")
        .env("FAIL_BUILD", "1")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(!project.target().join("debug/.vmux-dev-ready").exists());
    assert!(fs::symlink_metadata(project.target().join("release/Vmux Dev.app")).is_err());
}
