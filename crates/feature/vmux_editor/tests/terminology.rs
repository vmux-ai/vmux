use std::fs;
use std::path::PathBuf;

struct BrandTerm(String);

impl BrandTerm {
    fn browser() -> Self {
        Self(["chro", "me"].concat())
    }

    fn matches(&self, line: &str) -> bool {
        let lower = line.to_ascii_lowercase();
        let bytes = line.as_bytes();
        let mut offset = 0;
        while let Some(pos) = lower[offset..].find(&self.0) {
            let start = offset + pos;
            let boundary = start == 0
                || !bytes[start - 1].is_ascii_alphabetic()
                || (bytes[start - 1].is_ascii_lowercase() && bytes[start].is_ascii_uppercase());
            if boundary {
                return true;
            }
            offset = start + self.0.len();
        }
        false
    }
}

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
    let forbidden = BrandTerm::browser();
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
            if forbidden.matches(line) {
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

#[test]
fn brand_term_matcher_ignores_unrelated_words_like_monochrome() {
    let term = BrandTerm::browser();
    let lower = &term.0;
    let upper = lower.to_ascii_uppercase();
    let pascal = format!("{}{}", &upper[..1], &lower[1..]);
    assert!(!term.matches("const AGENT_MONOCHROME_REGISTRY: &str = \"\";"));
    assert!(!term.matches("fn is_monochrome_registry_svg(src: &str) -> bool"));
    assert!(!term.matches("Monochrome"));
    assert!(term.matches(&format!("{lower}_debug.log")));
    assert!(term.matches(&format!("{pascal}Runtime")));
    assert!(term.matches(&format!("struct My{pascal}Bridge;")));
    assert!(term.matches(&format!("const MY_{upper}_FLAG: bool = true;")));
}
