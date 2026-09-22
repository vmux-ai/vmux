use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED_PREFIXES: &[&str] = &[
    "crates/vmux_browser/src/extensions/",
    "crates/vmux_core/src/host/extension/",
];
const ALLOWED_FILES: &[&str] = &[
    "crates/host/vmux_mcp/src/tools/param.rs",
    "crates/vmux_browser/src/bin/vmux-extension-conformance.rs",
    "crates/vmux_browser/src/lib.rs",
    "crates/vmux_browser/src/page_life.rs",
];

#[test]
fn browser_brand_terms_stay_in_browser_compatibility_code() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();
    let mut rust_files = Vec::new();
    collect_rust_files(&root.join("crates"), &mut rust_files);
    let forbidden = ["chro", "me"].concat();
    let mut violations = Vec::new();
    for path in rust_files {
        let relative = path.strip_prefix(&root).unwrap().to_string_lossy();
        if ALLOWED_PREFIXES
            .iter()
            .any(|prefix| relative.starts_with(prefix))
            || ALLOWED_FILES.contains(&relative.as_ref())
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

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
