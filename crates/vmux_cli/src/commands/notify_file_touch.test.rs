use super::*;

#[test]
fn claude_read_with_offset() {
    let v = serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "/a/b.rs", "offset": 120 }
    });
    assert_eq!(
        parse_touch(&v),
        Some(("/a/b.rs".to_string(), Some(120), FileTouchKind::Read))
    );
}

#[test]
fn claude_edit_no_offset() {
    let v = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": { "file_path": "/a/b.rs", "old_string": "x", "new_string": "y" }
    });
    assert_eq!(
        parse_touch(&v),
        Some(("/a/b.rs".to_string(), None, FileTouchKind::Edit))
    );
}

#[test]
fn codex_apply_patch_is_edit() {
    let v = serde_json::json!({
        "tool_name": "apply_patch",
        "tool_input": { "file_path": "/a/b.rs" }
    });
    assert_eq!(parse_touch(&v).unwrap().2, FileTouchKind::Edit);
}

#[test]
fn vibe_lowercase_read() {
    let v = serde_json::json!({
        "tool_name": "read",
        "tool_input": { "file_path": "/a/b.rs" }
    });
    assert_eq!(parse_touch(&v).unwrap().2, FileTouchKind::Read);
}

#[test]
fn relative_path_skipped() {
    let v = serde_json::json!({ "tool_name": "Read", "tool_input": { "file_path": "b.rs" } });
    assert_eq!(parse_touch(&v), None);
}

#[test]
fn non_file_tool_skipped() {
    let v = serde_json::json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } });
    assert_eq!(parse_touch(&v), None);
}

#[test]
fn missing_tool_input_skipped() {
    let v = serde_json::json!({ "tool_name": "Read" });
    assert_eq!(parse_touch(&v), None);
}
