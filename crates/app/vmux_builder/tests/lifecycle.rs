#![cfg(unix)]

use std::fs;
use std::process::Command;

use support::{ChildGuard, Project};

pub mod support;

#[test]
fn completed_build_survives_a_cleanup_inspection_failure() {
    let project = Project::create();
    project.script("bin/lsof", "#!/bin/sh\nexit 1\n");
    let output = project.make("build-dev");
    assert!(
        project
            .target()
            .join("debug/Vmux Dev.app/Contents/MacOS/vmux_desktop")
            .is_file()
    );
    assert!(project.target().join("debug/deps/cache.o").is_file());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cleanup skipped after successful build")
    );
}

#[test]
fn build_lifecycle_preserves_artifacts_and_resolves_launch_paths() {
    let project = Project::create();
    for index in 1..=5 {
        project.write_target(
            &format!("release/Vmux ({index:07x}).app/Contents/MacOS/vmux_desktop"),
            "old signed",
        );
    }
    for failure in ["FAIL_PACKAGE", "FAIL_SIGNATURE"] {
        let output = project
            .command("make")
            .arg("build-local")
            .env(failure, "1")
            .output()
            .unwrap();
        assert!(!output.status.success());
        for index in 1..=5 {
            assert!(
                project
                    .target()
                    .join(format!(
                        "release/Vmux ({index:07x}).app/Contents/MacOS/vmux_desktop"
                    ))
                    .is_file()
            );
        }
    }
    project.make("build-dev");
    assert!(
        project
            .target()
            .join("debug/Vmux Dev.app/Contents/MacOS/vmux_desktop")
            .exists()
    );
    assert!(project.target().join("debug/deps/current.dylib").exists());
    assert!(!project.target().join("debug/deps/cache.o").exists());
    project.make("local");
    assert_eq!(
        project.opened(),
        project.target().join("release/Vmux (9999999).app")
    );
    let apps = fs::read_dir(project.target().join("release"))
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("Vmux (")
        })
        .count();
    assert_eq!(apps, 3);
    project.make("release");
    assert_eq!(project.opened(), project.target().join("release/Vmux.app"));
    assert_eq!(
        fs::read_to_string(project.target().join("release/Vmux.dmg")).unwrap(),
        "distribution\n"
    );
    for (goal, name) in [("local", "Vmux (9999999).app"), ("release", "Vmux.app")] {
        let output = project
            .command("make")
            .arg(goal)
            .env("CARGO_BUILD_TARGET", "fixture-target")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            project.opened(),
            project.target().join("fixture-target/release").join(name)
        );
    }
    for setting in ["relative output", ""] {
        let directory = project.root().join(if setting.is_empty() {
            "target"
        } else {
            setting
        });
        fs::create_dir(&directory).unwrap();
        fs::copy(
            project.target().join("CACHEDIR.TAG"),
            directory.join("CACHEDIR.TAG"),
        )
        .unwrap();
        let output = project
            .command("make")
            .arg("local")
            .arg(format!("CARGO_TARGET_DIR={setting}"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            project.opened(),
            directory.join("release/Vmux (9999999).app")
        );
    }
    for goal in ["build-dev", "build-local", "build-release"] {
        let log = fs::read(project.root().join("build.log")).unwrap();
        let output = project
            .command("make")
            .arg(goal)
            .env("VMUX_BUILD_STORAGE_MIN_GIB", "1000000")
            .env("VMUX_BUILD_STORAGE_WARN_GIB", "1000000")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(fs::read(project.root().join("build.log")).unwrap(), log);
    }
    let cache = project.write_target("debug/deps/cache.o", "cached");
    project.make("clean-build-cache");
    assert!(cache.exists());
    assert!(
        project
            .command("make")
            .arg("clean-build-cache")
            .env("VMUX_BUILD_STORAGE_APPLY", "1")
            .status()
            .unwrap()
            .success()
    );
    assert!(!cache.exists());
    assert!(project.target().join("debug/deps/current.dylib").exists());
}

#[test]
fn termination_reaches_the_child_and_releases_the_shell_compatible_lock() {
    let project = Project::create();
    let cache = project.write_target("debug/deps/cache.o", "keep after cancellation");
    let ready = project.root().join("child.pid");
    let mut command = project.command(env!("CARGO_BIN_EXE_vmux_builder"));
    command
        .arg("--root")
        .arg(project.root())
        .arg("dev")
        .args([
            "/bin/sh",
            "-c",
            "printf '%s' $$ > \"$1\"; exec sleep 30",
            "fixture",
        ])
        .arg(&ready);
    let mut child = command.spawn().unwrap();
    project.wait_for(&ready);
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(project.wait_child(&mut child).code(), Some(143));
    let pid = fs::read_to_string(ready).unwrap();
    assert!(
        !Command::new("kill")
            .args(["-0", &pid])
            .status()
            .unwrap()
            .success()
    );
    assert!(cache.exists());
    let mut shell = project.command("/bin/bash");
    shell.args(["-c", "source \"$1/scripts/target-cache-common.sh\"; vmux_target_lock_acquire \"$2\" \"$1\" 8 try; vmux_target_lock_release 8", "fixture"])
        .arg(project.repo()).arg(project.target());
    assert!(shell.status().unwrap().success());
}

#[test]
fn cancellation_does_not_wait_for_an_existing_shell_lock_owner() {
    let project = Project::create();
    let ready = project.root().join("holder.pid");
    let mut holder = project.command("/bin/bash");
    holder.args(["-ec", "source \"$1/scripts/target-cache-common.sh\"; vmux_target_lock_acquire \"$2\" \"$1\" 8; printf '%s' $$ > \"$3\"; exec sleep 30", "fixture"])
        .arg(project.repo()).arg(project.target()).arg(&ready);
    let _holder = ChildGuard::new(holder.spawn().unwrap());
    project.wait_for(&ready);
    let waiting = project.root().join("waiting.log");
    let mut command = project.command(env!("CARGO_BIN_EXE_vmux_builder"));
    command
        .arg("--root")
        .arg(project.root())
        .args(["dev", "/bin/true"])
        .stderr(fs::File::create(&waiting).unwrap());
    let mut child = command.spawn().unwrap();
    project.wait_output(&waiting);
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(project.wait_child(&mut child).code(), Some(143));
}
