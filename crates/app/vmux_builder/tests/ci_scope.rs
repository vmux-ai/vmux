#![cfg(unix)]

use std::fs;
use std::process::{Command, Output};

use support::Project;

pub mod support;

impl Project {
    fn scope_fixture() -> Self {
        let project = Self::create();
        project.script(
            "Cargo.toml",
            "[workspace]\nresolver = \"3\"\nmembers = [\"crates/feature/first\", \"crates/feature/second\", \"crates/feature/consumer\"]\n",
        );
        for name in ["first", "second", "consumer"] {
            project.script(
                &format!("crates/feature/{name}/Cargo.toml"),
                &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
            );
            project.script(
                &format!("crates/feature/{name}/src/lib.rs"),
                "pub struct Marker;\n",
            );
        }
        project.script(
            "crates/feature/consumer/Cargo.toml",
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\n[dev-dependencies]\nsecond = { path = \"../second\" }\n",
        );
        project.script("crates/feature/first/src/shared.rs", "pub struct First;\n");
        project.script(
            "bin/cargo",
            &format!(
                r##"#!/bin/sh
cat <<'METADATA'
{{"packages":[
  {{"name":"first","manifest_path":"{root}/crates/feature/first/Cargo.toml","dependencies":[]}},
  {{"name":"second","manifest_path":"{root}/crates/feature/second/Cargo.toml","dependencies":[]}},
  {{"name":"consumer","manifest_path":"{root}/crates/feature/consumer/Cargo.toml","dependencies":[{{"name":"second","kind":"dev","optional":false}}]}}
]}}
METADATA
"##,
                root = project.root().display(),
            ),
        );
        project.git(&["init"]);
        project.commit();
        project
    }

    fn git(&self, args: &[&str]) -> String {
        let mut command = Command::new("/usr/bin/git");
        command
            .args(args)
            .current_dir(self.root())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        for key in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
            command.env_remove(key);
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{:?}", output);
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn commit(&self) {
        self.git(&["add", "."]);
        self.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ]);
    }

    fn scope(&self, base: Option<&str>) -> Output {
        let mut command = self.command(env!("CARGO_BIN_EXE_vmux_builder"));
        command
            .arg("--root")
            .arg(self.root())
            .arg("ci-scope")
            .env("CARGO_BIN", self.root().join("bin/cargo"))
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env_remove("VMUX_CI_BASE");
        for key in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
            command.env_remove(key);
        }
        if let Some(base) = base {
            command.args(["--base", base]);
        }
        command.output().unwrap()
    }
}

#[test]
fn a_cross_package_rename_tests_both_owners_and_the_test_dependency_consumer() {
    let project = Project::scope_fixture();
    let base = project.git(&["rev-parse", "HEAD"]);
    let destination = project.root().join("crates/feature/second/src/shared.rs");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::rename(
        project.root().join("crates/feature/first/src/shared.rs"),
        destination,
    )
    .unwrap();
    project.commit();

    let output = project.scope(Some(&base));
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "args=-p consumer -p first -p second\nlint_args=-p consumer -p first -p second\nlean=false\n"
    );
}

#[test]
fn missing_comparison_runs_all_tests_and_invalid_history_fails_closed() {
    let project = Project::scope_fixture();
    let output = project.scope(None);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "args=--workspace\nlint_args=-p consumer -p first -p second\nlean=true\n"
    );

    let output = project.scope(Some("missing-commit"));
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn the_launcher_updates_an_old_builder_before_parsing_a_new_command() {
    let project = Project::scope_fixture();
    let sources = project.root().join("crates/app/vmux_builder/src");
    fs::create_dir_all(&sources).unwrap();
    for entry in fs::read_dir(project.repo().join("crates/app/vmux_builder/src")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), sources.join(entry.file_name())).unwrap();
    }
    let main = sources.join("main.rs");
    let current = fs::read_to_string(&main).unwrap();
    fs::write(&main, current.replace("ci-scope", "legacy-ci-scope")).unwrap();
    let output = project
        .command("./scripts/vmux-builder")
        .env_remove("VMUX_BUILDER_BIN")
        .arg("--print-bin")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);

    fs::write(main, current).unwrap();
    let output = project
        .command("./scripts/vmux-builder")
        .env_remove("VMUX_BUILDER_BIN")
        .env_remove("VMUX_CI_BASE")
        .env("CARGO_BIN", project.root().join("bin/cargo"))
        .arg("ci-scope")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "args=--workspace\nlint_args=-p consumer -p first -p second\nlean=true\n"
    );
}
