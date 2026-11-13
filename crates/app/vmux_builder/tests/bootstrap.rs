#![cfg(unix)]

use std::fs;

use support::Project;

pub mod support;

#[test]
fn host_bootstrap_is_cached_and_rebuilds_after_source_changes() {
    let project = Project::create();
    let sources = project.root().join("crates/app/vmux_builder/src");
    fs::create_dir_all(&sources).unwrap();
    fs::copy(
        project.repo().join("crates/build_product.rs"),
        project.root().join("crates/build_product.rs"),
    )
    .unwrap();
    for entry in fs::read_dir(project.repo().join("crates/app/vmux_builder/src")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), sources.join(entry.file_name())).unwrap();
    }
    project.script(
        "bin/rustc-probe",
        r#"#!/bin/sh
case "$*" in *--edition=2024*) printf 'compile\n' >> "$TEST_COMPILER_LOG" ;; esac
exec "$REAL_RUSTC" "$@"
"#,
    );
    let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let log = project.root().join("compiler.log");
    for expected in [1, 1, 2, 3] {
        if expected == 2 {
            let main = sources.join("main.rs");
            let mut text = fs::read_to_string(&main).unwrap();
            text.push('\n');
            fs::write(main, text).unwrap();
        }
        if expected == 3 {
            let metadata = project.root().join("crates/build_product.rs");
            let mut text = fs::read_to_string(&metadata).unwrap();
            text.push('\n');
            fs::write(metadata, text).unwrap();
        }
        let mut command = project.command("./scripts/vmux-builder");
        command
            .env_remove("VMUX_BUILDER_BIN")
            .env("RUSTC", project.root().join("bin/rustc-probe"))
            .env("REAL_RUSTC", &compiler)
            .env("TEST_COMPILER_LOG", &log)
            .env("CARGO_BUILD_TARGET", "fixture-target")
            .arg("status");
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), expected);
    }
}
