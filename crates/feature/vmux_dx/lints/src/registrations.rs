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
fn compiler_resolves_registered_systems_and_ignores_other_contracts() {
    let fixture = Fixture::new();
    for (name, source, expected) in [
        (
            "private",
            include_str!("../inputs/private.rs.input"),
            &[][..],
        ),
        (
            "unrelated",
            include_str!("../inputs/unrelated.rs.input"),
            &[][..],
        ),
        (
            "exported",
            include_str!("../inputs/exported.rs.input"),
            &["public", "crate_visible", "parent_visible"][..],
        ),
        (
            "aliases",
            include_str!("../inputs/aliases.rs.input"),
            &["exported"][..],
        ),
        (
            "adapters",
            include_str!("../inputs/adapters.rs.input"),
            &["source", "sink", "mapped"][..],
        ),
        (
            "qualified",
            include_str!("../inputs/qualified.rs.input"),
            &["exported"][..],
        ),
        (
            "reassigned",
            include_str!("../inputs/reassigned.rs.input"),
            &["exported"][..],
        ),
        (
            "overwritten",
            include_str!("../inputs/overwritten.rs.input"),
            &[][..],
        ),
        (
            "inside_branch",
            include_str!("../inputs/inside_branch.rs.input"),
            &[][..],
        ),
        (
            "branch_join",
            include_str!("../inputs/branch_join.rs.input"),
            &[][..],
        ),
        (
            "uninitialized",
            include_str!("../inputs/uninitialized.rs.input"),
            &["exported"][..],
        ),
        (
            "block_results",
            include_str!("../inputs/block_results.rs.input"),
            &["exported_block", "exported_if", "exported_match"][..],
        ),
    ] {
        fixture.check(name, source, expected);
    }
}
