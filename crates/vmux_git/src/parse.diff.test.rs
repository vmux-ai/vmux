use super::*;

const DIFF: &str = "diff --git a/f.rs b/f.rs\nindex 1..2 100644\n--- a/f.rs\n+++ b/f.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    let x = 1;\n+    let x = 2;\n }\n";

#[test]
fn skips_file_headers() {
    let lines = parse_unified_diff(DIFF);
    assert!(
        !lines
            .iter()
            .any(|l| l.spans[0].text.starts_with("diff --git"))
    );
    assert!(!lines.iter().any(|l| l.spans[0].text.starts_with("+++")));
}

#[test]
fn classifies_kinds_and_numbers() {
    let lines = parse_unified_diff(DIFF);
    let hunk = &lines[0];
    assert!(matches!(hunk.kind, DiffKind::Hunk));

    let ctx = &lines[1];
    assert!(matches!(ctx.kind, DiffKind::Context));
    assert_eq!(ctx.old_no, Some(1));
    assert_eq!(ctx.new_no, Some(1));

    let rem = &lines[2];
    assert!(matches!(rem.kind, DiffKind::Remove));
    assert_eq!(rem.old_no, Some(2));
    assert_eq!(rem.new_no, None);

    let add = &lines[3];
    assert!(matches!(add.kind, DiffKind::Add));
    assert_eq!(add.old_no, None);
    assert_eq!(add.new_no, Some(2));
}

#[test]
fn empty_diff_yields_no_lines() {
    assert!(parse_unified_diff("").is_empty());
}
