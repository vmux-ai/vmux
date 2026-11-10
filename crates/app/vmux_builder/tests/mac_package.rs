#![cfg(unix)]

use std::fs;

use support::Project;

pub mod support;

#[test]
fn temporary_signing_keychain_is_restored_after_success_and_import_failure() {
    for fail in [false, true] {
        let project = Project::create();
        project.script(
            "bin/security",
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$TEST_SECURITY_LOG"
case "$1" in
  list-keychains) [ "${4:-}" = -s ] || printf '"login.keychain-db"\n"system.keychain"\n' ;;
  find-identity) printf '1) fixture "fixture"\n' ;;
  import) [ "${FAIL_IMPORT:-0}" != 1 ] || exit 42 ;;
esac
"#,
        );
        let log = project.root().join("security.log");
        let mut command = project.command(env!("CARGO_BIN_EXE_vmux_builder"));
        command
            .arg("--root")
            .arg(project.root())
            .arg("prepare-local")
            .env("APPLE_CERTIFICATE", "QUJD")
            .env("APPLE_CERTIFICATE_PASSWORD", "fixture-password")
            .env("TEST_SECURITY_LOG", &log)
            .env("FAIL_IMPORT", if fail { "1" } else { "0" });
        let output = command.output().unwrap();
        assert_eq!(
            output.status.success(),
            !fail,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let lines = fs::read_to_string(log).unwrap();
        let entries: Vec<_> = lines.lines().collect();
        assert_eq!(
            entries[entries.len() - 2],
            "list-keychains -d user -s login.keychain-db system.keychain"
        );
        assert!(entries.last().unwrap().starts_with("delete-keychain "));
        let path = entries
            .last()
            .unwrap()
            .strip_prefix("delete-keychain ")
            .unwrap();
        assert!(!std::path::Path::new(path).parent().unwrap().exists());
        if fail {
            assert!(!project.root().join("build.log").exists());
        }
    }
}

#[test]
fn direct_package_entry_reads_env_before_selecting_the_target() {
    let project = Project::create();
    let target = project.root().join("configured output");
    fs::create_dir(&target).unwrap();
    fs::copy(
        project.target().join("CACHEDIR.TAG"),
        target.join("CACHEDIR.TAG"),
    )
    .unwrap();
    fs::write(
        project.root().join(".env"),
        format!(
            "# configuration\nCARGO_TARGET_DIR={}\nAPPLE_SIGNING_IDENTITY=fixture\n",
            target.display()
        ),
    )
    .unwrap();
    let output = project
        .command("./scripts/build-mac-release.sh")
        .arg("local")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        target
            .join("release/Vmux (9999999).app/Contents/MacOS/vmux_desktop")
            .is_file()
    );
    assert!(!project.target().join("release/Vmux (9999999).app").exists());
}
