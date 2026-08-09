use super::*;

#[test]
fn tool_activity_classifies_timeline_icons() {
    assert_eq!(ToolActivity::of("guardian_review"), ToolActivity::Guardian);
    assert_eq!(ToolActivity::of("read_file"), ToolActivity::ReadFile);
    assert_eq!(ToolActivity::of("apply_patch"), ToolActivity::WriteFile);
    assert_eq!(ToolActivity::of("read_layout"), ToolActivity::Layout);
    assert_eq!(ToolActivity::of("create_worktree"), ToolActivity::Worktree);
    assert_eq!(ToolActivity::of("select_project"), ToolActivity::Worktree);
    assert_eq!(ToolActivity::of("view_image"), ToolActivity::Image);
    assert_eq!(
        ToolActivity::of("vmux_screenshot"),
        ToolActivity::Screenshot
    );
    assert_eq!(ToolActivity::of("vmux_open_page"), ToolActivity::OpenPage);
    assert_eq!(ToolActivity::of("vmux_open_file"), ToolActivity::ReadFile);
    assert_eq!(ToolActivity::of("browser_navigate"), ToolActivity::Browser);
    assert_eq!(ToolActivity::of("search_files"), ToolActivity::Search);
    assert_eq!(ToolActivity::of("exec_command"), ToolActivity::Command);
    assert_eq!(ToolActivity::of("custom_tool"), ToolActivity::Other);
}

/// The arguments outrank the name: a `run` that executes Python is a Python call.
#[test]
fn a_tool_icon_prefers_the_language_in_its_arguments() {
    assert_eq!(
        ActivityIcon::for_tool("run", r#"{"cmd":"python main.py"}"#),
        ActivityIcon::Python
    );
    assert_eq!(
        ActivityIcon::for_tool("run", r#"{"cmd":"ls"}"#),
        ActivityIcon::Command
    );
}

#[test]
fn skill_reads_are_identified_from_nested_tool_arguments() {
    assert!(tool_args_read_skill(
        r#"{"arguments":{"path":"/tmp/skills/caveman/SKILL.md"},"server":"vmux","tool":"read_file"}"#
    ));
    assert!(!tool_args_read_skill(
        r#"{"arguments":{"path":"/tmp/src/lib.rs"},"server":"vmux","tool":"read_file"}"#
    ));
}
