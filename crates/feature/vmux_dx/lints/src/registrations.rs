use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(test)]
struct Fixture {
    directory: tempfile::TempDir,
    library: PathBuf,
}

#[cfg(test)]
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let library = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        std::fs::create_dir(directory.path().join("src")).unwrap();
        std::fs::create_dir(directory.path().join("examples")).unwrap();
        std::fs::write(directory.path().join("src/lib.rs"), "").unwrap();
        std::fs::copy(
            library.join("Cargo.toml"),
            directory.path().join("Cargo.toml"),
        )
        .unwrap();
        std::fs::copy(
            library.join("Cargo.lock"),
            directory.path().join("Cargo.lock"),
        )
        .unwrap();
        Self { directory, library }
    }

    fn check(&self, name: &str, source: &str, expected: &[&str]) {
        std::fs::write(
            self.directory
                .path()
                .join("examples")
                .join(format!("{name}.rs")),
            source,
        )
        .unwrap();
        let output = Command::new("cargo")
            .current_dir(self.directory.path())
            .args([
                "dylint",
                "--path",
                self.library.to_str().unwrap(),
                "--",
                "--locked",
                "--example",
                name,
                "--message-format=json",
            ])
            .output()
            .expect("run the prepared Dylint compiler");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        let mut diagnostics = Vec::new();
        let mut unexpected = Vec::new();
        for line in stdout.lines() {
            let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if event["reason"] != "compiler-message" {
                continue;
            }
            let message = &event["message"];
            if message["level"] != "error" {
                continue;
            }
            if message["code"]["code"] != "vmux_exported_system" {
                unexpected.push(message.to_string());
                continue;
            }
            let spans = message["spans"]
                .as_array()
                .expect("source-located lint diagnostic");
            assert!(
                spans.iter().any(|span| {
                    span["is_primary"] == true
                        && Path::new(span["file_name"].as_str().unwrap())
                            .file_stem()
                            .unwrap()
                            == name
                        && span["line_start"].as_u64().unwrap() > 0
                }),
                "{name}: {message}"
            );
            diagnostics.push(message["message"].as_str().unwrap().to_owned());
        }
        assert!(unexpected.is_empty(), "{name}: {unexpected:?}\n{stderr}");
        let mut expected: Vec<String> = expected
            .iter()
            .map(|name| {
                format!("registered system `{name}` must be private to its defining module")
            })
            .collect();
        expected.sort();
        diagnostics.sort();
        assert_eq!(diagnostics, expected, "{name}: {stderr}\n{stdout}");
        assert_eq!(
            output.status.success(),
            expected.is_empty(),
            "{name}: {stderr}\n{stdout}"
        );
    }
}

#[test]
fn compiler_resolves_observers_and_ignores_lookalike_registration_apis() {
    let fixture = Fixture::new();
    for (name, source, expected) in [
        (
            "observers",
            include_str!("../ui/observers.rs"),
            &[
                "public",
                "crate_visible",
                "parent_visible",
                "world_visible",
                "entity_visible",
                "commands_visible",
                "constructed",
            ][..],
        ),
        (
            "unrelated_observers",
            include_str!("../ui/unrelated_observers.rs"),
            &[][..],
        ),
    ] {
        fixture.check(name, source, expected);
    }
}

#[test]
fn compiler_resolves_registered_systems_and_ignores_other_contracts() {
    let fixture = Fixture::new();
    for (name, source, expected) in [
        ("private", include_str!("../ui/private.rs"), &[][..]),
        ("unrelated", include_str!("../ui/unrelated.rs"), &[][..]),
        (
            "exported",
            include_str!("../ui/exported.rs"),
            &["public", "crate_visible", "parent_visible"][..],
        ),
        (
            "aliases",
            include_str!("../ui/aliases.rs"),
            &["exported"][..],
        ),
        (
            "adapters",
            include_str!("../ui/adapters.rs"),
            &["source", "sink", "mapped"][..],
        ),
        (
            "qualified",
            include_str!("../ui/qualified.rs"),
            &["exported"][..],
        ),
        (
            "reassigned",
            include_str!("../ui/reassigned.rs"),
            &["exported"][..],
        ),
        ("overwritten", include_str!("../ui/overwritten.rs"), &[][..]),
        (
            "inside_branch",
            include_str!("../ui/inside_branch.rs"),
            &[][..],
        ),
        ("branch_join", include_str!("../ui/branch_join.rs"), &[][..]),
        (
            "uninitialized",
            include_str!("../ui/uninitialized.rs"),
            &["exported"][..],
        ),
        (
            "block_results",
            include_str!("../ui/block_results.rs"),
            &["exported_block", "exported_if", "exported_match"][..],
        ),
        (
            "loops",
            include_str!("../ui/loops.rs"),
            &["carried", "continued", "from_loop"][..],
        ),
    ] {
        fixture.check(name, source, expected);
    }
}
