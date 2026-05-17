use serde::Serialize;
use serde_json::Value;
use vmux_command::command::AppCommand;
use vmux_macro::McpTool;
use vmux_service::protocol::{AgentCommand, AgentShellMode};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, McpTool)]
pub enum McpParamTool {
    #[mcp(description = "Open the Vmux command bar.")]
    OpenCommandBar {
        #[mcp(enum_values = ["default", "commands", "path"])]
        mode: Option<String>,
    },
    #[mcp(
        description = "Spawn a process in a new visible Vmux tab. If `command` is omitted, the user's default shell is launched. If `args` is provided as a single string, it is split on whitespace. If `cwd` is omitted, the active space's directory (~/.vmux/<space>) is used. Useful for opening claude/vibe/codex/nvim/etc. directly without going through a shell."
    )]
    NewTerminalTab {
        cwd: Option<String>,
        command: Option<String>,
        args: Option<String>,
    },
    #[mcp(description = "Run a shell command in a visible Vmux terminal.")]
    RunShell {
        command: String,
        cwd: Option<String>,
        #[mcp(enum_values = ["new_tab", "active"])]
        mode: Option<String>,
    },
    #[mcp(
        description = "Navigate the active webview to a URL, or open a URL in a target pane. URLs starting with 'vmux://terminal/' open a terminal (use '?cwd=/path' to set working dir), 'vmux://spaces/' opens the spaces view, 'vmux://services/' opens the processes monitor; other 'vmux://' URLs are rejected; everything else opens as a browser. With 'vmux://' URLs, a new tab is always created in the target pane (defaulting to the focused pane)."
    )]
    BrowserNavigate { url: String, pane: Option<String> },
    #[mcp(description = "Send raw text to the active terminal (no carriage return appended).")]
    TerminalSend {
        text: String,
        terminal: Option<String>,
    },
    #[mcp(description = "Select a tab by index (1-8).")]
    SelectTab { index: u8 },
    #[mcp(
        description = "Split current pane and open a URL in the new pane. Direction 'right' = side-by-side (vertical separator), 'down' = top/bottom. URLs starting with 'vmux://terminal/' open a terminal (use '?cwd=/path' to set working dir), 'vmux://spaces/' opens the spaces view, 'vmux://services/' opens the processes monitor; other 'vmux://' URLs are rejected; everything else opens as a browser."
    )]
    SplitAndNavigate {
        #[mcp(enum_values = ["right", "down"])]
        direction: String,
        url: String,
    },
    #[mcp(description = "Update a single vmux setting by dot-path. \
            Example: { path: 'layout.pane.gap', value: 12 }. \
            Use get_settings to discover the available paths and current values. \
            For nested arrays, use bracket indexing like 'terminal.themes[0].font_size'.")]
    UpdateSettings {
        path: String,
        value: serde_json::Value,
    },
    #[mcp(description = "Navigate the active or specified browser pane back one page in history.")]
    BrowserGoBack { pane: Option<String> },
    #[mcp(
        description = "Navigate the active or specified browser pane forward one page in history."
    )]
    BrowserGoForward { pane: Option<String> },
    #[mcp(
        description = "Search vmux browsing history. Returns up to `limit` entries ranked by frecency."
    )]
    BrowserHistorySearch { query: String, limit: Option<u32> },
}

impl McpParamTool {
    pub fn to_agent_command(self) -> Result<AgentCommand, String> {
        match self {
            McpParamTool::OpenCommandBar { mode } => {
                let id = match mode.as_deref().unwrap_or("default") {
                    "default" => "browser_open_command_bar",
                    "commands" => "browser_open_commands",
                    "path" => "browser_open_path_bar",
                    other => return Err(format!("unknown command bar mode: {other}")),
                };
                Ok(AgentCommand::AppCommand { id: id.to_string() })
            }
            McpParamTool::NewTerminalTab { cwd, command, args } => {
                let args_vec = args
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                Ok(AgentCommand::NewTerminalTab {
                    cwd: cwd.unwrap_or_default(),
                    command: command.unwrap_or_default(),
                    args: args_vec,
                    env: vec![],
                })
            }
            McpParamTool::RunShell { command, cwd, mode } => {
                if command.trim().is_empty() {
                    return Err("run_shell.command is empty".to_string());
                }
                let mode = match mode.as_deref().unwrap_or("new_tab") {
                    "new_tab" => AgentShellMode::NewTab,
                    "active" => AgentShellMode::Active,
                    other => return Err(format!("unknown shell mode: {other}")),
                };
                Ok(AgentCommand::RunShell {
                    command,
                    cwd: cwd.unwrap_or_default(),
                    mode,
                })
            }
            McpParamTool::BrowserNavigate { url, pane } => {
                if url.trim().is_empty() {
                    return Err("browser_navigate.url is empty".to_string());
                }
                Ok(AgentCommand::BrowserNavigate { url, pane })
            }
            McpParamTool::TerminalSend { text, terminal } => {
                if text.is_empty() {
                    return Err("terminal_send.text is empty".to_string());
                }
                Ok(AgentCommand::TerminalSend { text, terminal })
            }
            McpParamTool::SelectTab { index } => {
                if !(1..=8).contains(&index) {
                    return Err(format!(
                        "select_tab.index must be between 1 and 8, got {index}"
                    ));
                }
                Ok(AgentCommand::AppCommand {
                    id: format!("tab_select_{index}"),
                })
            }
            McpParamTool::SplitAndNavigate { direction, url } => {
                if !["right", "down"].contains(&direction.as_str()) {
                    return Err(format!(
                        "split_and_navigate: direction must be 'right' or 'down', got '{direction}'"
                    ));
                }
                if url.trim().is_empty() {
                    return Err("split_and_navigate.url is empty".to_string());
                }
                Ok(AgentCommand::SplitAndNavigate { direction, url })
            }
            McpParamTool::UpdateSettings { path, value } => {
                if path.trim().is_empty() {
                    return Err("update_settings.path is empty".to_string());
                }
                Ok(AgentCommand::UpdateSettings {
                    path,
                    value_json: value.to_string(),
                })
            }
            McpParamTool::BrowserGoBack { pane } => Ok(AgentCommand::BrowserGoBack { pane }),
            McpParamTool::BrowserGoForward { pane } => Ok(AgentCommand::BrowserGoForward { pane }),
            McpParamTool::BrowserHistorySearch { query, limit } => {
                if query.trim().is_empty() {
                    return Err("browser_history_search.query is empty".into());
                }
                let limit = limit.unwrap_or(20).min(100);
                Ok(AgentCommand::BrowserHistorySearch { query, limit })
            }
        }
    }
}

#[derive(Debug, McpTool)]
pub enum McpQueryTool {
    #[mcp(description = "Return the full vmux layout snapshot (spaces, panes, tabs, focused).")]
    GetState,
    #[mcp(description = "List all tabs across all spaces with title, url, and kind.")]
    ListTabs,
    #[mcp(description = "List all spaces with their panes and tabs.")]
    ListSpaces,
    #[mcp(description = "List all terminal processes with cwd and pid.")]
    ListTerminals,
    #[mcp(description = "Return the currently focused space, pane, and tab ids.")]
    GetFocused,
    #[mcp(description = "Return the full vmux settings as a JSON snapshot.")]
    GetSettings,
}

impl McpQueryTool {
    pub fn to_agent_query(self) -> vmux_service::protocol::AgentQuery {
        use vmux_service::protocol::AgentQuery;
        match self {
            McpQueryTool::GetState => AgentQuery::GetState,
            McpQueryTool::ListTabs => AgentQuery::ListTabs,
            McpQueryTool::ListSpaces => AgentQuery::ListSpaces,
            McpQueryTool::ListTerminals => AgentQuery::ListTerminals,
            McpQueryTool::GetFocused => AgentQuery::GetFocused,
            McpQueryTool::GetSettings => AgentQuery::GetSettings,
        }
    }
}

#[derive(Debug)]
pub enum DispatchTarget {
    Command(AgentCommand),
    Query(vmux_service::protocol::AgentQuery),
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    AppCommand::mcp_tool_entries()
        .into_iter()
        .chain(McpParamTool::mcp_tool_entries())
        .chain(McpQueryTool::mcp_tool_entries())
        .map(|(name, description, schema)| ToolDefinition {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: schema,
        })
        .collect()
}

pub fn dispatch_from_tool_call(name: &str, arguments: Value) -> Result<DispatchTarget, String> {
    if let Some(parsed) = McpQueryTool::from_mcp_id(name) {
        return Ok(DispatchTarget::Query(parsed.to_agent_query()));
    }
    if let Some(parsed) = McpParamTool::from_mcp_call(name, arguments) {
        return parsed
            .and_then(McpParamTool::to_agent_command)
            .map(DispatchTarget::Command);
    }
    if AppCommand::from_mcp_id(name).is_some() {
        return Ok(DispatchTarget::Command(AgentCommand::AppCommand {
            id: name.to_string(),
        }));
    }
    Err(format!("unknown tool: {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::{AgentCommand, AgentQuery};

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

    #[test]
    fn list_tools_includes_auto_generated_and_handwritten() {
        let names = tool_names();

        for hand in ["open_command_bar", "new_terminal_tab", "run_shell"] {
            assert!(
                names.contains(&hand.to_string()),
                "missing hand-written {hand}"
            );
        }
        for auto in [
            "stack_new",
            "stack_close",
            "new_tab",
            "close_tab",
            "split_v",
            "terminal_clear",
            "browser_reload",
        ] {
            assert!(
                names.contains(&auto.to_string()),
                "missing auto-generated {auto}"
            );
        }
    }

    #[test]
    fn auto_generated_tool_dispatches_as_app_command() {
        let command = dispatch_command("split_v", serde_json::json!({})).unwrap();
        assert_eq!(
            command,
            AgentCommand::AppCommand {
                id: "split_v".to_string()
            }
        );
    }

    #[test]
    fn unknown_tool_returns_error() {
        assert!(dispatch_from_tool_call("nope_not_a_tool", serde_json::json!({})).is_err());
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
    fn empty_run_shell_command_returns_tool_error() {
        assert!(dispatch_from_tool_call("run_shell", serde_json::json!({"command": ""})).is_err());
    }

    #[test]
    fn list_tools_includes_terminal_send() {
        let names = tool_names();
        assert!(names.contains(&"terminal_send".to_string()));
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
    fn terminal_send_missing_text_returns_error() {
        assert!(dispatch_from_tool_call("terminal_send", serde_json::json!({})).is_err());
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
            }
        );
    }

    #[test]
    fn select_tab_out_of_range_returns_error() {
        assert!(dispatch_from_tool_call("select_tab", serde_json::json!({"index": 0})).is_err());
        assert!(dispatch_from_tool_call("select_tab", serde_json::json!({"index": 9})).is_err());
    }

    #[test]
    fn tool_list_includes_query_tools() {
        let names = tool_names();
        for query in [
            "get_state",
            "list_tabs",
            "list_spaces",
            "list_terminals",
            "get_focused",
        ] {
            assert!(
                names.contains(&query.to_string()),
                "missing query tool {query}"
            );
        }
    }

    #[test]
    fn mcp_param_tool_entries_includes_all_param_tools() {
        let names: Vec<&'static str> = McpParamTool::mcp_tool_entries()
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
        for expected in [
            "open_command_bar",
            "new_terminal_tab",
            "run_shell",
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
        let result = McpParamTool::from_mcp_call("browser_navigate", serde_json::json!({}))
            .expect("recognized");
        assert!(result.is_err());
    }

    #[test]
    fn mcp_param_tool_from_mcp_call_unknown_returns_none() {
        assert!(McpParamTool::from_mcp_call("nope", serde_json::json!({})).is_none());
    }

    #[test]
    fn mcp_query_tool_entries_includes_all_query_tools() {
        let names: Vec<&'static str> = McpQueryTool::mcp_tool_entries()
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
        for expected in [
            "get_state",
            "list_tabs",
            "list_spaces",
            "list_terminals",
            "get_focused",
        ] {
            assert!(names.contains(&expected), "missing query tool {expected}");
        }
    }

    #[test]
    fn dispatch_from_tool_call_routes_command() {
        let target = dispatch_from_tool_call("split_v", serde_json::json!({})).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Command(AgentCommand::AppCommand { id }) if id == "split_v"
        ));
    }

    #[test]
    fn dispatch_from_tool_call_routes_query() {
        let target = dispatch_from_tool_call("get_state", serde_json::json!({})).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(AgentQuery::GetState)
        ));
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
    fn mcp_param_tool_entries_includes_split_and_navigate() {
        let names: Vec<&'static str> = McpParamTool::mcp_tool_entries()
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
        assert!(names.contains(&"split_and_navigate"));
    }

    #[test]
    fn split_and_navigate_dispatches_to_agent_command() {
        let target = dispatch_from_tool_call(
            "split_and_navigate",
            serde_json::json!({"direction": "right", "url": "https://example.com"}),
        )
        .unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Command(AgentCommand::SplitAndNavigate { direction, url })
                if direction == "right" && url == "https://example.com"
        ));
    }

    #[test]
    fn split_and_navigate_invalid_direction_returns_error() {
        let result = dispatch_from_tool_call(
            "split_and_navigate",
            serde_json::json!({"direction": "sideways", "url": "https://example.com"}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn split_and_navigate_missing_url_returns_error() {
        let result = dispatch_from_tool_call(
            "split_and_navigate",
            serde_json::json!({"direction": "right"}),
        );
        assert!(result.is_err());
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
}
