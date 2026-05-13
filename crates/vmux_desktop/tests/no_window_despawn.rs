use std::path::Path;

#[test]
fn no_production_code_inserts_closing_window() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = crate_root.join("src");

    let mut violations = Vec::new();
    walk(&src, &mut |path, contents| {
        for (i, line) in contents.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if trimmed.contains("insert(ClosingWindow)")
                || trimmed.contains("try_insert(ClosingWindow)")
            {
                violations.push(format!("{}:{} → {}", path.display(), i + 1, trimmed));
            }
        }
    });

    assert!(
        violations.is_empty(),
        "Found ClosingWindow inserts that should be window-hide instead:\n{}",
        violations.join("\n")
    );
}

fn walk(dir: &Path, f: &mut dyn FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs")
            && let Ok(contents) = std::fs::read_to_string(&path)
        {
            f(&path, &contents);
        }
    }
}
