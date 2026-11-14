#![cfg(all(unix, feature = "ci-syntax"))]

use std::process::{Command, Output};

use std::path::{Path, PathBuf};
use std::os::unix::fs::PermissionsExt;

struct Project {
    directory: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn create() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("checkout with spaces");
        std::fs::create_dir_all(&root).unwrap();
        let project = Self { directory, root };
        project.script("crates/app/vmux_cli/Cargo.toml", "[package]\nname = \"vmux_cli\"\nversion = \"0.0.35\"\n");
        project
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn script(&self, name: &str, contents: &str) {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command.current_dir(&self.root);
        for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR", "CARGO_BUILD_TARGET"] {
            command.env_remove(name);
        }
        command.env("CARGO_TARGET_DIR", self.directory.path().join("target"));
        command
    }
}

impl Project {
    fn native_fixture() -> Self {
        let project = Self::create();
        project.script(
            "crates/feature/sample/src/lib.rs",
            "pub fn live() {} #[cfg(test)] mod tests { #[test] fn check() { assert!(true); } }\n",
        );
        project.script("docs/architecture.md", "fixture documentation\n");
        project.script("ecs-policy.ron", "fixture policy\n");
        project.script(
            "crates/feature/sample/Cargo.toml",
            "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n",
        );
        project.script("bin/cargo", &format!(r#"#!/bin/sh
cat <<'METADATA'
{{"packages":[{{"manifest_path":"{root}/crates/feature/sample/Cargo.toml","targets":[{{"kind":["lib"],"src_path":"{root}/crates/feature/sample/src/lib.rs"}}]}}]}}
METADATA
"#, root = project.root().display()));
        project.native_git(&["init", "-b", "main"]);
        project.native_commit();
        project
    }

    fn native_git(&self, args: &[&str]) -> String {
        let mut command = Command::new("/usr/bin/git");
        command
            .args(args)
            .current_dir(self.root())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
        ] {
            command.env_remove(key);
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{:?}", output);
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn native_commit(&self) {
        self.native_git(&["add", "."]);
        self.native_git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "fixture",
        ]);
    }

    fn native(&self, base: Option<&str>) -> Output {
        let mut command = self.command(env!("CARGO_BIN_EXE_vmux"));
        command
            .args(["ci-native", "--root", self.root().to_str().unwrap(), "--"])
            .env("CARGO_BIN", self.root().join("bin/cargo"))
            .env_remove("VMUX_CI_BASE");
        if let Some(base) = base {
            command.args(["--base", base]);
        }
        command.output().unwrap()
    }
}

#[test]
fn portable_tests_and_their_policy_updates_skip_native_builds_but_production_edits_do_not() {
    let project = Project::native_fixture();
    let base = project.native_git(&["rev-parse", "HEAD"]);
    project.script(
        "crates/feature/sample/src/lib.rs",
        "pub fn live() {} #[cfg(test)] mod tests { #[test] fn check() { assert_eq!(1, 1); } }\n",
    );
    project.script("docs/architecture.md", "updated fixture documentation\n");
    project.script("ecs-policy.ron", "updated fixture policy\n");
    project.native_commit();
    let output = project.native(Some(&base));
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"native=false\n");

    project.script(
        "crates/feature/sample/src/lib.rs",
        "pub fn live() { changed(); } #[cfg(test)] mod tests { #[test] fn check() { assert_eq!(1, 1); } }\n",
    );
    project.native_commit();
    let output = project.native(Some(&base));
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"native=true\n");
}

#[test]
fn missing_history_and_unparseable_or_build_inputs_cannot_disable_native_checks() {
    let project = Project::native_fixture();
    let base = project.native_git(&["rev-parse", "HEAD"]);
    let output = project.native(None);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"native=true\n");
    let output = project.native(Some("missing-commit"));
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());

    for (path, contents) in [
        ("crates/feature/sample/src/lib.rs", "fn invalid(\n"),
        (
            "crates/feature/sample/build.rs",
            "#[cfg(test)] fn fixture() {}\n",
        ),
        (
            "crates/build_platform_cfg.rs",
            "#[cfg(test)] fn fixture() {}\n",
        ),
        (
            "crates/feature/sample/Cargo.toml",
            "[package]\nname = \"sample\"\n",
        ),
        (
            "crates/app/vmux_builder/src/main.rs",
            "#[cfg(test)] fn fixture() {}\n",
        ),
    ] {
        project.native_git(&["reset", "--hard", &base]);
        project.native_git(&["clean", "-fd"]);
        project.script(path, contents);
        project.native_commit();
        let output = project.native(Some(&base));
        assert!(output.status.success(), "{path}: {:?}", output);
        assert_eq!(output.stdout, b"native=true\n", "{path}");
    }
}

#[test]
fn source_mode_changes_retain_native_validation() {
    let project = Project::native_fixture();
    let base = project.native_git(&["rev-parse", "HEAD"]);
    project.native_git(&[
        "update-index",
        "--chmod=-x",
        "crates/feature/sample/src/lib.rs",
    ]);
    project.native_git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-m",
        "source mode",
    ]);
    let output = project.native(Some(&base));
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"native=true\n");
}

#[test]
fn native_parent_modules_cannot_hide_changed_child_tests_from_linux() {
    let project = Project::native_fixture();
    project.script(
        "crates/feature/sample/src/lib.rs",
        "#[cfg(target_os = \"macos\")] mod native;\n",
    );
    project.script(
        "crates/feature/sample/src/native.rs",
        "#[cfg(test)] mod tests { #[test] fn check() { assert!(true); } }\n",
    );
    project.native_commit();
    let base = project.native_git(&["rev-parse", "HEAD"]);
    project.script(
        "crates/feature/sample/src/native.rs",
        "#[cfg(test)] mod tests { #[test] fn check() { assert!(false); } }\n",
    );
    project.native_commit();
    let output = project.native(Some(&base));
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"native=true\n");
}

#[test]
fn inherited_coverage_is_proven_or_native_validation_is_retained() {
    for (declaration, source, native) in [
        ("mod native;", "native.rs", false),
        ("#[cfg(host)] mod native;", "native.rs", false),
        ("#[cfg(unix)] mod native;", "native.rs", false),
        ("#[cfg(not(host))] mod native;", "native.rs", true),
        (
            "#[cfg(feature = \"package\")] mod native;",
            "native.rs",
            true,
        ),
        (
            "#[cfg_attr(unix, cfg(target_os = \"macos\"))] mod native;",
            "native.rs",
            true,
        ),
        (
            "mod outer { #[cfg(target_arch = \"aarch64\")] mod native; }",
            "outer/native.rs",
            true,
        ),
        ("#[path = \"native.rs\"] mod alias;", "native.rs", true),
        ("", "native.rs", true),
    ] {
        let project = Project::native_fixture();
        let path = format!("crates/feature/sample/src/{source}");
        project.script("crates/feature/sample/src/lib.rs", declaration);
        project.script(
            &path,
            "#[cfg(test)] mod tests { #[test] fn check() { assert!(true); } }\n",
        );
        project.native_commit();
        let base = project.native_git(&["rev-parse", "HEAD"]);
        project.script(
            &path,
            "#[cfg(test)] mod tests { #[test] fn check() { assert!(false); } }\n",
        );
        project.native_commit();
        let output = project.native(Some(&base));
        assert!(output.status.success(), "{declaration}: {:?}", output);
        assert_eq!(
            output.stdout,
            format!("native={native}\n").as_bytes(),
            "{declaration}"
        );
    }
}
