use std::fs;
use std::path::PathBuf;

#[test]
fn browser_brand_terms_stay_in_browser_compatibility_code() {
    let allowed_prefixes = ["crates/feature/vmux_extension/"];
    let allowed_files = [
        "crates/util/vmux_mcp/src/tools/param.rs",
        "crates/util/vmux_browser/src/lib.rs",
        "crates/util/vmux_browser/src/page_life.rs",
    ];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();
    let mut rust_files = Vec::new();
    let mut pending = vec![root.join("crates")];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                rust_files.push(path);
            }
        }
    }
    let forbidden = ["chro", "me"].concat();
    let mut violations = Vec::new();
    for path in rust_files {
        let relative = path.strip_prefix(&root).unwrap().to_string_lossy();
        if allowed_prefixes
            .iter()
            .any(|prefix| relative.starts_with(prefix))
            || allowed_files.contains(&relative.as_ref())
            || matches!(
                relative.as_ref(),
                "crates/feature/vmux_dx/src/native/cef_source.rs"
                    | "crates/feature/vmux_dx/src/native/conformance.rs"
            )
        {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        for (index, line) in source.lines().enumerate() {
            if line.to_ascii_lowercase().contains(&forbidden) {
                violations.push(format!("{relative}:{}: {line}", index + 1));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "reserved browser-brand term used outside compatibility code:\n{}",
        violations.join("\n")
    );
}
