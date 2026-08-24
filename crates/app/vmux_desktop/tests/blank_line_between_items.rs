use std::path::Path;

const ITEM_KEYWORDS: &[&str] = &[
    "pub ",
    "pub(",
    "fn ",
    "struct ",
    "enum ",
    "impl ",
    "trait ",
    "union ",
    "const ",
    "static ",
    "type ",
    "mod ",
    "use ",
    "macro_rules!",
    "unsafe fn ",
    "unsafe impl ",
    "unsafe trait ",
    "async fn ",
    "extern crate ",
];

#[test]
fn every_item_is_separated_from_the_body_above_it() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir");

    let mut violations = Vec::new();
    walk(crates_dir, &mut |path, source| {
        if path.ends_with("blank_line_between_items.rs") {
            return;
        }
        for line in offending_lines(source) {
            violations.push(format!("{}:{line}", path.display()));
        }
    });

    assert!(
        violations.is_empty(),
        "these items start immediately after a closing brace — put one blank line between \
         them:\n{}",
        violations.join("\n")
    );
}

fn offending_lines(source: &str) -> Vec<usize> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate().skip(1) {
        let indent = line.len() - line.trim_start().len();
        if lines[index - 1] != format!("{}}}", " ".repeat(indent)) {
            continue;
        }
        if starts_an_item(&lines, index) {
            found.push(index + 1);
        }
    }
    found
}

fn starts_an_item(lines: &[&str], index: usize) -> bool {
    for line in lines.iter().skip(index) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[") || trimmed.starts_with("///") {
            continue;
        }
        return ITEM_KEYWORDS
            .iter()
            .any(|keyword| trimmed.starts_with(keyword));
    }
    false
}

fn walk(dir: &Path, visit: &mut dyn FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("target") {
                continue;
            }
            walk(&path, visit);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs")
            && let Ok(source) = std::fs::read_to_string(&path)
        {
            visit(&path, &source);
        }
    }
}

#[test]
fn the_scan_catches_an_item_jammed_under_a_brace() {
    let jammed = "fn a() {\n    todo!()\n}\nfn b() {}\n";
    assert_eq!(offending_lines(jammed), vec![4]);

    let attributed = "impl A {\n}\n#[cfg(test)]\nmod tests {}\n";
    assert_eq!(offending_lines(attributed), vec![3]);

    let nested = "mod outer {\n    fn a() {\n    }\n    fn b() {}\n}\n";
    assert_eq!(offending_lines(nested), vec![4]);
}

#[test]
fn the_scan_leaves_packed_declarations_and_statements_alone() {
    let separated = "fn a() {\n    todo!()\n}\n\nfn b() {}\n";
    assert!(offending_lines(separated).is_empty());

    let declarations = "mod a;\nmod b;\npub use a::A;\nuse b::B;\n";
    assert!(offending_lines(declarations).is_empty());

    let attribute_on_declaration = "pub mod sheet;\n#[cfg(web)]\npub mod sidebar;\n";
    assert!(offending_lines(attribute_on_declaration).is_empty());

    let wrapped_use = "use a::{\n    B, C,\n};\nuse d::E;\n";
    assert!(offending_lines(wrapped_use).is_empty());

    let wrapped_const = "const X: F = F {\n    a: 1,\n};\nconst Y: u8 = 2;\n";
    assert!(offending_lines(wrapped_const).is_empty());

    let cfg_blocks = "fn a() {\n    #[cfg(unix)]\n    {\n        one()\n    }\n    #[cfg(not(unix))]\n    {\n        two()\n    }\n}\n";
    assert!(offending_lines(cfg_blocks).is_empty());

    let blocks =
        "fn a() {\n    unsafe {\n        one()\n    }\n    unsafe {\n        two()\n    }\n}\n";
    assert!(offending_lines(blocks).is_empty());

    let guards = "fn a() {\n    let Some(x) = y else {\n        return;\n    };\n    if x {\n        return;\n    }\n    go(x);\n}\n";
    assert!(offending_lines(guards).is_empty());

    let arms = "fn a() {\n    match x {\n        A => {\n            one()\n        }\n        B => two(),\n    }\n}\n";
    assert!(offending_lines(arms).is_empty());
}
