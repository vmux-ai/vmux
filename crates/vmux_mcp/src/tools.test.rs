use super::*;
use vmux_client::protocol::{AgentCommand, AgentQuery};

fn tool_names() -> Vec<String> {
    tool_definitions()
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

fn dispatch_command(name: &str, args: serde_json::Value) -> Result<AgentCommand, String> {
    match dispatch_from_tool_call(name, args)? {
        DispatchTarget::Command(cmd) => Ok(cmd),
        DispatchTarget::Query(_) => Err("expected Command, got Query".to_string()),
    }
}

fn dispatch_query(name: &str, args: serde_json::Value) -> Result<AgentQuery, String> {
    match dispatch_from_tool_call(name, args)? {
        DispatchTarget::Query(q) => Ok(q),
        DispatchTarget::Command(_) => Err("expected Query, got Command".to_string()),
    }
}

#[test]
fn record_tools_are_listed() {
    let names = tool_names();
    assert!(names.contains(&"record_start".to_string()));
    assert!(names.contains(&"record_stop".to_string()));
}

#[test]
fn browser_snapshot_dispatches_to_query_with_pane() {
    let q = dispatch_query(
        "browser_snapshot",
        serde_json::json!({ "target": "pane:42" }),
    )
    .unwrap();
    assert_eq!(
        q,
        AgentQuery::BrowserSnapshot {
            pane: Some("pane:42".to_string()),
            anchor: None,
        }
    );
}

#[test]
fn browser_snapshot_defaults_pane_to_none() {
    let q = dispatch_query("browser_snapshot", serde_json::json!({})).unwrap();
    assert_eq!(
        q,
        AgentQuery::BrowserSnapshot {
            pane: None,
            anchor: None,
        }
    );
}

#[test]
fn browser_snapshot_is_listed() {
    assert!(tool_names().contains(&"browser_snapshot".to_string()));
}

#[test]
fn browser_snapshot_rejects_non_string_target() {
    let err = dispatch_query("browser_snapshot", serde_json::json!({ "target": 123 })).unwrap_err();
    assert!(err.contains("target"));
}

#[test]
fn browser_scroll_dispatches_with_delta() {
    let q = dispatch_query("browser_scroll", serde_json::json!({ "delta": 600 })).unwrap();
    assert_eq!(
        q,
        AgentQuery::BrowserScroll {
            pane: None,
            to: None,
            delta: Some(600),
            anchor: None,
        }
    );
}

#[test]
fn browser_scroll_dispatches_to_bottom_with_pane() {
    let q = dispatch_query(
        "browser_scroll",
        serde_json::json!({ "to": "bottom", "target": "pane:3" }),
    )
    .unwrap();
    assert_eq!(
        q,
        AgentQuery::BrowserScroll {
            pane: Some("pane:3".to_string()),
            to: Some("bottom".to_string()),
            delta: None,
            anchor: None,
        }
    );
}

#[test]
fn browser_scroll_requires_exactly_one_of_to_or_delta() {
    assert!(dispatch_query("browser_scroll", serde_json::json!({})).is_err());
    assert!(
        dispatch_query(
            "browser_scroll",
            serde_json::json!({ "to": "top", "delta": 5 })
        )
        .is_err()
    );
}

#[test]
fn browser_scroll_rejects_non_integer_or_out_of_range_delta() {
    let err = dispatch_query("browser_scroll", serde_json::json!({ "delta": "600" })).unwrap_err();
    assert!(err.contains("delta must be an integer"));
    let err = dispatch_query(
        "browser_scroll",
        serde_json::json!({ "delta": 5_000_000_000i64 }),
    )
    .unwrap_err();
    assert!(err.contains("out of range"));
}

#[test]
fn browser_scroll_is_listed() {
    assert!(tool_names().contains(&"browser_scroll".to_string()));
}

#[test]
fn install_extension_is_listed() {
    assert!(tool_names().contains(&"browser_install_extension".to_string()));
}

#[test]
fn install_extension_dispatches_with_source() {
    let cmd = dispatch_command(
        "browser_install_extension",
        serde_json::json!({ "source": "cjpalhdlnbpafiamejdnhcphjbkeiagm" }),
    )
    .unwrap();
    assert_eq!(
        cmd,
        AgentCommand::BrowserInstallExtension {
            source: "cjpalhdlnbpafiamejdnhcphjbkeiagm".to_string()
        }
    );
}

#[test]
fn install_extension_rejects_empty_source() {
    let err = dispatch_command(
        "browser_install_extension",
        serde_json::json!({ "source": "  " }),
    )
    .unwrap_err();
    assert!(err.contains("source"));
}

#[test]
fn record_start_dispatch_defaults() {
    let q = dispatch_query("record_start", serde_json::json!({})).unwrap();
    assert_eq!(
        q,
        AgentQuery::RecordStart {
            gif: false,
            max_secs: 600,
            pane: None
        }
    );
}

#[test]
fn record_start_dispatch_args() {
    let q = dispatch_query(
        "record_start",
        serde_json::json!({"gif": true, "max_secs": 30, "pane": "pane:3"}),
    )
    .unwrap();
    assert_eq!(
        q,
        AgentQuery::RecordStart {
            gif: true,
            max_secs: 30,
            pane: Some("pane:3".into())
        }
    );
}

#[test]
fn record_stop_dispatch_args() {
    let q = dispatch_query(
        "record_stop",
        serde_json::json!({"dir": "/tmp/out", "name": "feature-x"}),
    )
    .unwrap();
    assert_eq!(
        q,
        AgentQuery::RecordStop {
            dir: Some("/tmp/out".into()),
            name: Some("feature-x".into())
        }
    );
    let empty = dispatch_query("record_stop", serde_json::json!({})).unwrap();
    assert_eq!(
        empty,
        AgentQuery::RecordStop {
            dir: None,
            name: None
        }
    );
}

#[test]
fn list_tools_includes_auto_generated_and_handwritten() {
    let names = tool_names();

    for hand in [
        "open_command_bar",
        "open_page",
        "run",
        "read_terminal",
        "request_user_choice",
        "vault_status",
        "open_vault",
        "set_conversation_title",
        "write_knowledge",
        "select_project",
        "create_worktree",
    ] {
        assert!(
            names.contains(&hand.to_string()),
            "missing hand-written {hand}"
        );
    }
    for removed_tool in [
        "new_terminal_tab",
        "run_shell",
        "in_pane",
        "select_workspace",
    ] {
        assert!(
            !names.contains(&removed_tool.to_string()),
            "superseded tool {removed_tool} should no longer appear in MCP tools"
        );
    }
    for auto in ["terminal_clear", "browser_reload"] {
        assert!(
            names.contains(&auto.to_string()),
            "missing auto-generated {auto}"
        );
    }
    assert!(
        names.iter().all(|n| !n.starts_with("vmux_")),
        "MCP tool names must not be vmux_-prefixed (server is already named vmux): {names:?}"
    );
    for removed in ["stack_new", "close_tab", "split_v"] {
        assert!(
            !names.contains(&removed.to_string()),
            "layout command {removed} should no longer appear in MCP tools"
        );
    }
}

#[test]
fn pane_open_tool_descriptions_prefer_auto_placement() {
    let defs = tool_definitions();
    let open_page = defs.iter().find(|tool| tool.name == "open_page").unwrap();
    let open_file = defs.iter().find(|tool| tool.name == "open_file").unwrap();
    let run = defs.iter().find(|tool| tool.name == "run").unwrap();

    assert!(open_page.description.contains("Omit `direction`"));
    assert!(open_file.description.contains("Omit `direction`"));
    assert!(run.description.contains("Omit `direction`"));
}

#[test]
fn auto_generated_tool_dispatches_as_app_command() {
    let command = dispatch_command("terminal_clear", serde_json::json!({})).unwrap();
    assert_eq!(
        command,
        AgentCommand::AppCommand {
            id: "terminal_clear".to_string(),
            args_json: String::new(),
        }
    );
}

#[test]
fn unknown_tool_returns_error() {
    assert!(dispatch_from_tool_call("nope_not_a_tool", serde_json::json!({})).is_err());
}

#[test]
fn list_tools_includes_notify() {
    assert!(tool_names().contains(&"notify".to_string()));
}

#[test]
fn notify_dispatches_to_notify_command() {
    let command = dispatch_command(
        "notify",
        serde_json::json!({"title": "done", "body": "built X"}),
    )
    .unwrap();
    assert_eq!(
        command,
        AgentCommand::Notify {
            title: Some("done".to_string()),
            body: Some("built X".to_string()),
        }
    );
}

#[test]
fn notify_allows_empty_args() {
    let command = dispatch_command("notify", serde_json::json!({})).unwrap();
    assert_eq!(
        command,
        AgentCommand::Notify {
            title: None,
            body: None,
        }
    );
}

#[test]
fn list_tools_includes_browser_navigate() {
    let names = tool_names();
    assert!(names.contains(&"browser_navigate".to_string()));
}

#[test]
fn browser_navigate_dispatches_with_url() {
    let command = dispatch_command(
        "browser_navigate",
        serde_json::json!({"url": "https://example.com"}),
    )
    .unwrap();
    assert_eq!(
        command,
        AgentCommand::BrowserNavigate {
            url: "https://example.com".to_string(),
            pane: None,
        }
    );
}

#[test]
fn browser_navigate_missing_url_returns_error() {
    assert!(dispatch_from_tool_call("browser_navigate", serde_json::json!({})).is_err());
}

#[test]
fn vmux_prefixed_tool_name_dispatches() {
    let command = dispatch_command(
        "vmux_browser_navigate",
        serde_json::json!({"url": "https://example.com"}),
    )
    .unwrap();
    assert_eq!(
        command,
        AgentCommand::BrowserNavigate {
            url: "https://example.com".to_string(),
            pane: None,
        }
    );
}

#[test]
fn list_tools_includes_terminal_send() {
    let names = tool_names();
    assert!(names.contains(&"terminal_send".to_string()));
}

#[test]
fn acp_terminals_toolset_hides_run_and_read_terminal_keeps_send() {
    let names: Vec<String> = tool_definitions_filtered(true, true)
        .into_iter()
        .map(|def| def.name)
        .collect();
    assert!(!names.contains(&"run".to_string()));
    assert!(!names.contains(&"read_terminal".to_string()));
    assert!(names.contains(&"terminal_send".to_string()));
    assert!(names.contains(&"open_page".to_string()));
    assert!(!names.contains(&"resume_in_acp".to_string()));
}

#[test]
fn cli_toolset_lists_resume_in_acp() {
    assert!(tool_names().contains(&"resume_in_acp".to_string()));
}

#[test]
fn resume_in_acp_dispatches_with_anchor() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let target =
        dispatch_with_anchor("resume_in_acp", serde_json::json!({}), Some(anchor)).unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Command(AgentCommand::ResumeInAcp { anchor: got }) if got == anchor
    ));
    assert!(dispatch_from_tool_call("resume_in_acp", serde_json::json!({})).is_err());
}

#[test]
fn conversation_title_dispatches_model_summary_to_agent_session() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "set_conversation_title",
        serde_json::json!({"title": "  Refine model-generated summaries  "}),
        Some(anchor),
    )
    .unwrap();

    assert!(matches!(
        target,
        DispatchTarget::Command(AgentCommand::SetConversationTitle { anchor: got, title })
            if got == anchor && title == "Refine model-generated summaries"
    ));
    assert!(
        dispatch_from_tool_call(
            "set_conversation_title",
            serde_json::json!({"title": "summary"})
        )
        .is_err()
    );
}

#[test]
fn knowledge_write_dispatches_validated_note_to_host() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "write_knowledge",
        serde_json::json!({
            "path": "projects/yc.md",
            "title": "YC Startup School",
            "content": "Notes"
        }),
        Some(anchor),
    )
    .unwrap();

    assert!(matches!(
        target,
        DispatchTarget::Command(AgentCommand::WriteKnowledge {
            anchor: got,
            path: Some(path),
            title,
            content,
        }) if got == anchor
            && path == "projects/yc.md"
            && title == "YC Startup School"
            && content == "Notes"
    ));
}

#[test]
fn knowledge_read_tools_dispatch_with_bounds_and_anchor() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let search = dispatch_with_anchor(
        "search_knowledge",
        serde_json::json!({"query": "  Obsidian links  ", "limit": 12}),
        Some(anchor),
    )
    .unwrap();
    let read = dispatch_with_anchor(
        "read_knowledge",
        serde_json::json!({"path": "projects/obsidian-gap-analysis.md", "line": 8}),
        Some(anchor),
    )
    .unwrap();

    assert!(matches!(
        search,
        DispatchTarget::Command(AgentCommand::SearchKnowledge {
            anchor: got,
            query,
            limit: 12,
        }) if got == anchor && query == "Obsidian links"
    ));
    assert!(matches!(
        read,
        DispatchTarget::Command(AgentCommand::ReadKnowledge {
            anchor: got,
            path,
            line: 8,
            limit: 200,
        }) if got == anchor && path == "projects/obsidian-gap-analysis.md"
    ));
    assert!(
        dispatch_from_tool_call("search_knowledge", serde_json::json!({"query": "links"})).is_err()
    );
}

#[test]
fn project_tools_dispatch_with_anchor_and_branch() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let worktree_definition = create_worktree_definition();
    let select_definition = select_project_definition();
    let choice_definition = request_user_choice_definition();
    assert!(
        worktree_definition
            .description
            .contains("Never call for requests that only read")
    );
    assert!(
        select_definition
            .description
            .contains("Never call for requests that only read")
    );
    assert!(
        select_definition
            .description
            .contains("Do not call for general questions")
    );
    assert!(
        select_definition
            .description
            .contains("Do not search the user's home directory")
    );
    assert!(
        select_definition
            .description
            .contains("native project picker")
    );
    assert!(
        select_definition
            .description
            .contains("~/.vmux/workspace/<remote-host>")
    );
    assert!(
        select_definition
            .description
            .contains("~/.vmux/workspace/local/<project>")
    );
    assert!(select_definition.description.contains("empty directory"));
    assert!(
        select_definition
            .description
            .contains("without a linked worktree")
    );
    assert!(
        select_definition
            .description
            .contains("returns immediately")
    );
    assert!(choice_definition.description.contains("~/.vmux/workspace"));
    assert!(choice_definition.description.contains("Ctrl+N/Ctrl+P"));
    let choose =
        dispatch_with_anchor("select_project", serde_json::json!({}), Some(anchor)).unwrap();
    let choose_path = dispatch_with_anchor(
        "select_project",
        serde_json::json!({"path": "/repo"}),
        Some(anchor),
    )
    .unwrap();
    let create = dispatch_with_anchor(
        "create_worktree",
        serde_json::json!({"branch": "feature/fun-terminal"}),
        Some(anchor),
    )
    .unwrap();
    let prepare = dispatch_with_anchor(
        "create_worktree",
        serde_json::json!({"path": "/repo-wt", "task": "fun terminal", "create": false}),
        Some(anchor),
    )
    .unwrap();
    let choice = dispatch_with_anchor(
        "request_user_choice",
        serde_json::json!({
            "question": "Worktree?",
            "options": ["Create new worktree", "/repo/.worktrees/feature"]
        }),
        Some(anchor),
    )
    .unwrap();

    assert!(matches!(
        choose,
        DispatchTarget::Command(AgentCommand::ChooseWorkspace { anchor: got }) if got == anchor
    ));
    assert!(matches!(
        choose_path,
        DispatchTarget::Command(AgentCommand::ChooseWorkspaceAtPath { anchor: got, path })
            if got == anchor && path == "/repo"
    ));
    assert!(matches!(
        create,
        DispatchTarget::Command(AgentCommand::CreateWorktreeOnBranch { anchor: got, branch })
            if got == anchor && branch == "feature/fun-terminal"
    ));
    assert!(matches!(
        prepare,
        DispatchTarget::Command(AgentCommand::PrepareWorktree { anchor: got, path, task, create })
            if got == anchor
                && path.as_deref() == Some("/repo-wt")
                && task.as_deref() == Some("fun terminal")
                && !create
    ));
    assert!(matches!(
        choice,
        DispatchTarget::Command(AgentCommand::RequestUserChoice { anchor: got, question, options })
            if got == anchor && question == "Worktree?" && options.len() == 2
    ));
}

#[test]
fn terminal_send_dispatches_with_text() {
    let command = dispatch_command("terminal_send", serde_json::json!({"text": "ls"})).unwrap();
    assert_eq!(
        command,
        AgentCommand::TerminalSend {
            text: "ls".to_string(),
            terminal: None,
        }
    );
}

#[test]
fn terminal_send_enter_appends_carriage_return() {
    let command = dispatch_command(
        "terminal_send",
        serde_json::json!({"text": "ls", "enter": true}),
    )
    .unwrap();
    assert_eq!(
        command,
        AgentCommand::TerminalSend {
            text: "ls\r".to_string(),
            terminal: None,
        }
    );
}

#[test]
fn terminal_send_enter_with_empty_text_submits_carriage_return() {
    let command = dispatch_command(
        "terminal_send",
        serde_json::json!({"text": "", "enter": true}),
    )
    .unwrap();
    assert_eq!(
        command,
        AgentCommand::TerminalSend {
            text: "\r".to_string(),
            terminal: None,
        }
    );
}

#[test]
fn terminal_send_missing_text_returns_error() {
    assert!(dispatch_from_tool_call("terminal_send", serde_json::json!({})).is_err());
}

#[test]
fn rename_profile_dispatches_with_name() {
    let command =
        dispatch_command("rename_profile", serde_json::json!({"name": "Junichi"})).unwrap();
    assert_eq!(
        command,
        AgentCommand::RenameProfile {
            name: "Junichi".to_string()
        }
    );
}

#[test]
fn rename_profile_empty_name_returns_error() {
    assert!(dispatch_from_tool_call("rename_profile", serde_json::json!({"name": "  "})).is_err());
}

#[test]
fn list_tools_includes_select_tab() {
    let names = tool_names();
    assert!(names.contains(&"select_tab".to_string()));
}

#[test]
fn select_tab_dispatches_to_tab_select_id() {
    let command = dispatch_command("select_tab", serde_json::json!({"index": 3})).unwrap();
    assert_eq!(
        command,
        AgentCommand::AppCommand {
            id: "tab_select_3".to_string(),
            args_json: String::new(),
        }
    );
}

#[test]
fn select_tab_out_of_range_returns_error() {
    assert!(dispatch_from_tool_call("select_tab", serde_json::json!({"index": 0})).is_err());
    assert!(dispatch_from_tool_call("select_tab", serde_json::json!({"index": 9})).is_err());
}

#[test]
fn tool_list_includes_read_and_update_layout() {
    let names = tool_names();
    assert!(names.contains(&"read_layout".to_string()));
    assert!(names.contains(&"update_layout".to_string()));
}

#[test]
fn list_tools_includes_screenshot() {
    assert!(tool_names().contains(&"screenshot".to_string()));
}

#[test]
fn screenshot_dispatches_to_query_with_and_without_pane() {
    let target = dispatch_from_tool_call("screenshot", serde_json::json!({})).unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Query(vmux_client::protocol::AgentQuery::Screenshot { pane: None })
    ));

    let target =
        dispatch_from_tool_call("screenshot", serde_json::json!({ "pane": "stack:7" })).unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Query(vmux_client::protocol::AgentQuery::Screenshot { pane: Some(p) })
            if p == "stack:7"
    ));

    let target =
        dispatch_from_tool_call("screenshot", serde_json::json!({ "pane": "  " })).unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Query(vmux_client::protocol::AgentQuery::Screenshot { pane: None })
    ));

    assert!(dispatch_from_tool_call("screenshot", serde_json::json!({ "pane": 123 })).is_err());
}

#[test]
fn mcp_param_tool_entries_includes_all_param_tools() {
    let names: Vec<&'static str> = McpParamTool::mcp_tool_entries()
        .into_iter()
        .map(|(name, _, _)| name)
        .collect();
    for expected in [
        "open_command_bar",
        "browser_navigate",
        "terminal_send",
        "select_tab",
    ] {
        assert!(names.contains(&expected), "missing param tool {expected}");
    }
}

#[test]
fn mcp_param_tool_browser_navigate_schema_marks_url_required() {
    let entry = McpParamTool::mcp_tool_entries()
        .into_iter()
        .find(|(name, _, _)| *name == "browser_navigate")
        .expect("browser_navigate present");
    let schema = entry.2;
    let required = schema.get("required").expect("required key");
    assert_eq!(required, &serde_json::json!(["url"]));
    let properties = schema.get("properties").expect("properties key");
    assert!(properties.get("url").is_some());
    assert!(properties.get("pane").is_some());
}

#[test]
fn mcp_param_tool_from_mcp_call_browser_navigate() {
    let parsed = McpParamTool::from_mcp_call(
        "browser_navigate",
        serde_json::json!({"url": "https://example.com", "pane": "12345"}),
    )
    .expect("recognized")
    .expect("parsed");
    assert!(matches!(
        parsed,
        McpParamTool::BrowserNavigate { url, pane: Some(p) }
            if url == "https://example.com" && p == "12345"
    ));
}

#[test]
fn mcp_param_tool_from_mcp_call_browser_navigate_missing_url_errors() {
    let result =
        McpParamTool::from_mcp_call("browser_navigate", serde_json::json!({})).expect("recognized");
    assert!(result.is_err());
}

#[test]
fn mcp_param_tool_from_mcp_call_unknown_returns_none() {
    assert!(McpParamTool::from_mcp_call("nope", serde_json::json!({})).is_none());
}

#[test]
fn dispatch_from_tool_call_routes_command() {
    let target = dispatch_from_tool_call("terminal_clear", serde_json::json!({})).unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Command(AgentCommand::AppCommand { id, .. }) if id == "terminal_clear"
    ));
}

#[test]
fn dispatch_read_layout_routes_to_query() {
    let target = dispatch_from_tool_call("read_layout", serde_json::json!({})).unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Query(AgentQuery::ReadLayout { .. })
    ));
}

#[test]
fn open_page_without_direction_is_auto() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "open_page",
        serde_json::json!({"url": "https://x.com"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::OpenBeside { direction, .. }) => {
            assert_eq!(direction, None, "absent direction => auto placement");
        }
        other => panic!("expected OpenBeside, got {other:?}"),
    }
}

#[test]
fn open_page_default_does_not_request_focus() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "open_page",
        serde_json::json!({"url": "https://x.com"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::OpenBeside { focus, .. }) => {
            assert!(!focus);
        }
        other => panic!("expected OpenBeside, got {other:?}"),
    }
}

#[test]
fn open_file_default_does_not_request_focus() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "open_file",
        serde_json::json!({"path": "/tmp/example.rs"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::OpenBeside { focus, .. }) => {
            assert!(!focus);
        }
        other => panic!("expected OpenBeside, got {other:?}"),
    }
}

#[test]
fn open_page_with_direction_is_explicit() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "open_page",
        serde_json::json!({"url": "https://x.com", "direction": "left"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::OpenBeside { direction, .. }) => {
            assert_eq!(
                direction,
                Some(vmux_client::protocol::AgentPaneDirection::Left)
            );
        }
        other => panic!("expected OpenBeside, got {other:?}"),
    }
}

#[test]
fn open_page_dispatch_uses_anchor() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "open_page",
        serde_json::json!({"direction": "right", "url": "vmux://terminal/"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::OpenBeside { anchor: a, url, .. }) => {
            assert_eq!(a, anchor);
            assert_eq!(url, "vmux://terminal/");
        }
        other => panic!("expected OpenBeside, got {other:?}"),
    }
    assert!(
        dispatch_with_anchor("open_page", serde_json::json!({"url": ""}), Some(anchor)).is_err()
    );
    assert!(dispatch_with_anchor("open_page", serde_json::json!({"url": "x"}), None).is_err());
    assert!(tool_definitions().iter().any(|d| d.name == "open_page"));
    assert!(tool_definitions().iter().any(|d| d.name == "run"));
    assert!(tool_definitions().iter().any(|d| d.name == "read_file"));
    assert!(tool_definitions().iter().any(|d| d.name == "grep"));
}

#[test]
fn open_vault_dispatch_focuses_confirmed_provider() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "open_vault",
        serde_json::json!({"provider": "github"}),
        Some(anchor),
    )
    .unwrap();

    assert!(matches!(
        target,
        DispatchTarget::Command(AgentCommand::OpenBeside {
            anchor: got,
            direction: None,
            url,
            focus: true,
        }) if got == anchor && url == "vmux://vault/?provider=github"
    ));
    assert!(
        dispatch_with_anchor(
            "open_vault",
            serde_json::json!({"provider": "unknown"}),
            Some(anchor),
        )
        .is_err()
    );
    assert!(dispatch_with_anchor("open_vault", serde_json::json!({}), None).is_err());
}

#[test]
fn run_dispatch_uses_anchor() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "run",
        serde_json::json!({"command": "echo hi"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::Run {
            anchor: a, command, ..
        }) => {
            assert_eq!(a, anchor);
            assert_eq!(command, "echo hi");
        }
        other => panic!("expected Run, got {other:?}"),
    }
    assert!(
        dispatch_with_anchor("run", serde_json::json!({"command": " "}), Some(anchor)).is_err()
    );
    assert!(dispatch_with_anchor("run", serde_json::json!({"command": "x"}), None).is_err());
}

#[test]
fn run_dispatch_tracks_explicit_placement_override() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let bare = dispatch_with_anchor(
        "run",
        serde_json::json!({"command": "echo hi"}),
        Some(anchor),
    )
    .unwrap();
    match bare {
        DispatchTarget::Command(AgentCommand::Run { .. }) => {}
        other => panic!("expected Run, got {other:?}"),
    }

    let nulls = dispatch_with_anchor(
        "run",
        serde_json::json!({
            "command": "echo hi",
            "mode": null,
            "direction": null,
            "beside": null
        }),
        Some(anchor),
    )
    .unwrap();
    match nulls {
        DispatchTarget::Command(AgentCommand::Run { .. }) => {}
        other => panic!("expected Run for null placement values, got {other:?}"),
    }

    for arguments in [
        serde_json::json!({"command": "echo hi", "direction": "bottom"}),
        serde_json::json!({"command": "echo hi", "mode": "auto"}),
        serde_json::json!({"command": "echo hi", "beside": "self"}),
    ] {
        let explicit = dispatch_with_anchor("run", arguments, Some(anchor)).unwrap();
        match explicit {
            DispatchTarget::Command(AgentCommand::RunWithPlacementOverride { .. }) => {}
            other => panic!("expected RunWithPlacementOverride, got {other:?}"),
        }
    }
}

#[test]
fn run_tool_documents_default_placement_policy() {
    let run = tool_definitions()
        .into_iter()
        .find(|definition| definition.name == "run")
        .expect("run definition");
    assert!(
        run.description
            .contains("agent.allow_run_placement_override")
    );
    assert!(
        run.description
            .contains("omit `mode`, `direction`, and `beside`")
    );
}

#[test]
fn run_with_terminal_targets_existing() {
    let anchor = vmux_client::protocol::ProcessId::new();
    let term = vmux_client::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "run",
        serde_json::json!({"command": "ls", "terminal": term.to_string()}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::Run {
            terminal: Some(t), ..
        }) => {
            assert_eq!(t, term);
        }
        other => panic!("expected Run with terminal, got {other:?}"),
    }
    assert!(
        dispatch_with_anchor(
            "run",
            serde_json::json!({"command": "ls", "terminal": "nope"}),
            Some(anchor)
        )
        .is_err()
    );
}

#[test]
fn run_beside_and_mode_dispatch() {
    use vmux_client::protocol::PlacementMode;
    let anchor = vmux_client::protocol::ProcessId::new();
    let beside = vmux_client::protocol::ProcessId::new();

    // beside=<id> + mode=stack carries through.
    let target = dispatch_with_anchor(
        "run",
        serde_json::json!({"command": "ls", "beside": beside.to_string(), "mode": "stack"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::RunWithPlacementOverride {
            beside: Some(b),
            mode,
            ..
        }) => {
            assert_eq!(b, beside);
            assert_eq!(mode, PlacementMode::Stack);
        }
        other => panic!("expected RunWithPlacementOverride with beside+stack, got {other:?}"),
    }

    // beside="self" => None; mode defaults to Auto (reuse the region).
    let target = dispatch_with_anchor(
        "run",
        serde_json::json!({"command": "ls", "beside": "self"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::RunWithPlacementOverride {
            beside: None,
            mode,
            ..
        }) => assert_eq!(mode, PlacementMode::Auto),
        other => panic!("expected RunWithPlacementOverride with self+auto, got {other:?}"),
    }

    // explicit mode=split is honored.
    let target = dispatch_with_anchor(
        "run",
        serde_json::json!({"command": "ls", "mode": "split"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::RunWithPlacementOverride { mode, .. }) => {
            assert_eq!(mode, PlacementMode::Split)
        }
        other => panic!("expected RunWithPlacementOverride with split, got {other:?}"),
    }

    // unknown mode errors.
    assert!(
        dispatch_with_anchor(
            "run",
            serde_json::json!({"command": "ls", "mode": "nope"}),
            Some(anchor),
        )
        .is_err()
    );
}

#[test]
fn read_terminal_dispatch_routes_to_query() {
    let pid = vmux_client::protocol::ProcessId::new();
    let target = dispatch_from_tool_call(
        "read_terminal",
        serde_json::json!({"terminal": pid.to_string()}),
    )
    .unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Query(vmux_client::protocol::AgentQuery::ReadTerminal { .. })
    ));
    assert!(
        dispatch_from_tool_call("read_terminal", serde_json::json!({"terminal": "bad"})).is_err()
    );
    assert!(tool_definitions().iter().any(|d| d.name == "read_terminal"));
}

#[test]
fn dispatch_update_layout_parses_payload() {
    let payload = serde_json::json!({
        "tabs": [{
            "id": "tab:1",
            "name": "Work",
            "is_active": true,
            "root": { "kind": "pane", "id": "pane:2", "stacks": [{ "id": "stack:3" }] }
        }],
        "focused": { "tab": "tab:1", "pane": "pane:2", "stack": "stack:3" }
    });
    let target = dispatch_from_tool_call("update_layout", payload).unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Command(AgentCommand::UpdateLayout { .. })
    ));
}

#[test]
fn dispatch_update_layout_rejects_malformed_payload() {
    let payload = serde_json::json!({ "not_a_layout": true });
    assert!(dispatch_from_tool_call("update_layout", payload).is_err());
}

#[test]
fn dispatch_from_tool_call_routes_param_command_with_pane() {
    let target = dispatch_from_tool_call(
        "browser_navigate",
        serde_json::json!({"url": "https://example.com", "pane": "12345"}),
    )
    .unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Command(AgentCommand::BrowserNavigate { url, pane: Some(p) })
            if url == "https://example.com" && p == "12345"
    ));
}

#[test]
fn dispatch_from_tool_call_unknown_returns_error() {
    assert!(dispatch_from_tool_call("nope", serde_json::json!({})).is_err());
}

#[test]
fn list_tools_includes_update_settings_and_get_settings() {
    let names = tool_names();
    assert!(names.contains(&"update_settings".to_string()));
    assert!(names.contains(&"get_settings".to_string()));
}

#[test]
fn update_settings_dispatches_with_path_and_value() {
    let target = dispatch_from_tool_call(
        "update_settings",
        serde_json::json!({"path": "layout.pane.gap", "value": 12.0}),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::UpdateSettings { path, value_json }) => {
            assert_eq!(path, "layout.pane.gap");
            let parsed: serde_json::Value = serde_json::from_str(&value_json).unwrap();
            assert_eq!(parsed, serde_json::json!(12.0));
        }
        other => panic!("expected UpdateSettings command, got {other:?}"),
    }
}

#[test]
fn update_settings_empty_path_returns_error() {
    let result = dispatch_from_tool_call(
        "update_settings",
        serde_json::json!({"path": "", "value": 1}),
    );
    assert!(result.is_err());
}

#[test]
fn get_settings_dispatches_to_query() {
    let target = dispatch_from_tool_call("get_settings", serde_json::json!({})).unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Query(AgentQuery::GetSettings)
    ));
}

#[test]
fn list_spaces_dispatches_to_query() {
    let target = dispatch_from_tool_call("list_spaces", serde_json::json!({})).unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Query(AgentQuery::ListSpaces)
    ));
}

#[test]
fn bookmark_list_dispatches_to_query() {
    let target = dispatch_from_tool_call("bookmark_list", serde_json::json!({})).unwrap();
    assert!(matches!(
        target,
        DispatchTarget::Query(AgentQuery::BookmarkList)
    ));
}

#[test]
fn bookmark_add_dispatches_to_command() {
    let cmd = dispatch_command(
        "bookmark_add",
        serde_json::json!({"url": "https://a.test", "title": "A", "folder": "f1"}),
    )
    .unwrap();
    match cmd {
        AgentCommand::BookmarkCommand {
            command,
            url,
            title,
            uuid,
            ..
        } => {
            assert_eq!(command, "add");
            assert_eq!(url.as_deref(), Some("https://a.test"));
            assert_eq!(title.as_deref(), Some("A"));
            assert_eq!(uuid.as_deref(), Some("f1"));
        }
        other => panic!("expected BookmarkCommand, got {other:?}"),
    }
}

#[test]
fn bookmark_folder_create_dispatches_to_command() {
    let cmd =
        dispatch_command("bookmark_folder_create", serde_json::json!({"name": "PRs"})).unwrap();
    match cmd {
        AgentCommand::BookmarkCommand { command, name, .. } => {
            assert_eq!(command, "folder_create");
            assert_eq!(name.as_deref(), Some("PRs"));
        }
        other => panic!("expected BookmarkCommand, got {other:?}"),
    }
}

#[test]
fn rename_space_dispatches_to_space_command() {
    let target = dispatch_from_tool_call(
        "rename_space",
        serde_json::json!({"space_id": "work", "name": "Client A"}),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::SpaceCommand {
            command,
            space_id,
            name,
        }) => {
            assert_eq!(command, "rename");
            assert_eq!(space_id.as_deref(), Some("work"));
            assert_eq!(name.as_deref(), Some("Client A"));
        }
        other => panic!("expected SpaceCommand, got {other:?}"),
    }
}

#[test]
fn create_space_dispatches_to_space_command() {
    let target =
        dispatch_from_tool_call("create_space", serde_json::json!({"name": "Work"})).unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::SpaceCommand { command, name, .. }) => {
            assert_eq!(command, "new");
            assert_eq!(name.as_deref(), Some("Work"));
        }
        other => panic!("expected SpaceCommand, got {other:?}"),
    }
}

#[test]
fn delete_space_empty_id_returns_error() {
    let result = dispatch_from_tool_call("delete_space", serde_json::json!({"space_id": ""}));
    assert!(result.is_err());
}

#[test]
fn open_command_tools_are_exposed() {
    let names = tool_names();
    for expected in ["in_place", "in_new_stack", "in_new_tab", "in_new_space"] {
        assert!(
            names.contains(&expected.to_string()),
            "missing OpenCommand tool: {expected}"
        );
    }
    assert!(
        !names.contains(&"in_pane".to_string()),
        "in_pane is hidden, superseded by open_page"
    );
}

#[test]
fn go_back_dispatches() {
    let r = McpParamTool::BrowserGoBack { pane: None }.to_agent_command();
    assert!(matches!(r, Ok(AgentCommand::BrowserGoBack { .. })));
}

#[test]
fn go_forward_dispatches() {
    let r = McpParamTool::BrowserGoForward { pane: None }.to_agent_command();
    assert!(matches!(r, Ok(AgentCommand::BrowserGoForward { .. })));
}

#[test]
fn history_search_rejects_empty_query() {
    let r = McpParamTool::BrowserHistorySearch {
        query: "  ".into(),
        limit: None,
    }
    .to_agent_command();
    assert!(r.is_err());
}

#[test]
fn history_search_clamps_limit() {
    let r = McpParamTool::BrowserHistorySearch {
        query: "x".into(),
        limit: Some(500),
    }
    .to_agent_command();
    match r {
        Ok(AgentCommand::BrowserHistorySearch { limit, .. }) => assert_eq!(limit, 100),
        _ => panic!(),
    }
}

#[test]
fn history_search_default_limit() {
    let r = McpParamTool::BrowserHistorySearch {
        query: "x".into(),
        limit: None,
    }
    .to_agent_command();
    match r {
        Ok(AgentCommand::BrowserHistorySearch { limit, .. }) => assert_eq!(limit, 20),
        _ => panic!(),
    }
}
