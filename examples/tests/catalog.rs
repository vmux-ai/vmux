use std::collections::BTreeSet;
use std::path::Path;

#[test]
fn catalog_matches_cargo_manifest_and_readme() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();

    let mut declared = BTreeSet::new();
    let mut in_example = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed == "[[example]]" {
            in_example = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_example = false;
            continue;
        }
        if in_example
            && let Some(name) = trimmed
                .strip_prefix("name = \"")
                .and_then(|rest| rest.strip_suffix('"'))
        {
            declared.insert(name.to_string());
        }
    }

    let listed = vmux_examples::EXAMPLES
        .iter()
        .map(|name| (*name).to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(declared, listed);

    for name in &listed {
        let command = format!("cargo run -p vmux_examples --example {name}");
        assert!(
            readme.contains(&command),
            "examples/README.md is missing `{command}`"
        );
    }
}
