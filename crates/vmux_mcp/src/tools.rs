use serde::Serialize;
use serde_json::{Value, json};
use vmux_command::command::AppCommand;
use vmux_service::protocol::{AgentCommand, AgentShellMode};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    let mut tools: Vec<ToolDefinition> = AppCommand::agent_entries()
        .into_iter()
        .map(|(id, description)| ToolDefinition {
            name: id.to_string(),
            description: description.to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        })
        .collect();

    tools.push(ToolDefinition {
        name: "open_command_bar".to_string(),
        description: "Open the Vmux command bar.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["default", "commands", "path"]
                }
            }
        }),
    });
    tools.push(ToolDefinition {
        name: "new_terminal_tab".to_string(),
        description: "Create a visible Vmux terminal tab.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "cwd": {
                    "type": "string"
                }
            }
        }),
    });
    tools.push(ToolDefinition {
        name: "run_shell".to_string(),
        description: "Run a shell command in a visible Vmux terminal.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string"
                },
                "cwd": {
                    "type": "string"
                },
                "mode": {
                    "type": "string",
                    "enum": ["new_tab", "active"]
                }
            },
            "required": ["command"]
        }),
    });

    tools.push(ToolDefinition {
        name: "browser_navigate".to_string(),
        description: "Navigate the active webview to a URL.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string"
                }
            },
            "required": ["url"]
        }),
    });

    tools
}

pub fn agent_command_from_tool_call(name: &str, arguments: Value) -> Result<AgentCommand, String> {
    match name {
        "open_command_bar" => {
            let mode = optional_string(&arguments, "mode").unwrap_or("default");
            let id = match mode {
                "default" => "browser_open_command_bar",
                "commands" => "browser_open_commands",
                "path" => "browser_open_path_bar",
                other => return Err(format!("unknown command bar mode: {other}")),
            };
            Ok(AgentCommand::AppCommand { id: id.to_string() })
        }
        "new_terminal_tab" => Ok(AgentCommand::NewTerminalTab {
            cwd: optional_string(&arguments, "cwd")
                .unwrap_or_default()
                .to_string(),
        }),
        "run_shell" => {
            let command = optional_string(&arguments, "command")
                .ok_or_else(|| "run_shell.command is required".to_string())?
                .to_string();
            if command.trim().is_empty() {
                return Err("run_shell.command is empty".to_string());
            }
            let mode = match optional_string(&arguments, "mode").unwrap_or("new_tab") {
                "new_tab" => AgentShellMode::NewTab,
                "active" => AgentShellMode::Active,
                other => return Err(format!("unknown shell mode: {other}")),
            };
            Ok(AgentCommand::RunShell {
                command,
                cwd: optional_string(&arguments, "cwd")
                    .unwrap_or_default()
                    .to_string(),
                mode,
            })
        }
        "browser_navigate" => {
            let url = optional_string(&arguments, "url")
                .ok_or_else(|| "browser_navigate.url is required".to_string())?
                .to_string();
            if url.trim().is_empty() {
                return Err("browser_navigate.url is empty".to_string());
            }
            Ok(AgentCommand::BrowserNavigate { url })
        }
        other => {
            if AppCommand::from_agent_id(other).is_some() {
                Ok(AgentCommand::AppCommand {
                    id: other.to_string(),
                })
            } else {
                Err(format!("unknown tool: {other}"))
            }
        }
    }
}

fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::AgentCommand;

    fn tool_names() -> Vec<String> {
        tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect()
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
            "tab_new",
            "tab_close",
            "split_v",
            "terminal_clear",
            "browser_reload",
        ] {
            assert!(
                names.contains(&auto.to_string()),
                "missing auto-generated {auto}"
            );
        }
        assert!(
            !names.contains(&"new_tab".to_string()),
            "removed hand-written new_tab should not appear"
        );
    }

    #[test]
    fn auto_generated_tool_dispatches_as_app_command() {
        let command = agent_command_from_tool_call("split_v", serde_json::json!({})).unwrap();
        assert_eq!(
            command,
            AgentCommand::AppCommand {
                id: "split_v".to_string()
            }
        );
    }

    #[test]
    fn unknown_tool_returns_error() {
        assert!(agent_command_from_tool_call("nope_not_a_tool", serde_json::json!({})).is_err());
    }

    #[test]
    fn list_tools_includes_browser_navigate() {
        let names = tool_names();
        assert!(names.contains(&"browser_navigate".to_string()));
    }

    #[test]
    fn browser_navigate_dispatches_with_url() {
        let command = agent_command_from_tool_call(
            "browser_navigate",
            serde_json::json!({"url": "https://example.com"}),
        )
        .unwrap();
        assert_eq!(
            command,
            AgentCommand::BrowserNavigate {
                url: "https://example.com".to_string(),
            }
        );
    }

    #[test]
    fn browser_navigate_missing_url_returns_error() {
        assert!(agent_command_from_tool_call("browser_navigate", serde_json::json!({})).is_err());
    }

    #[test]
    fn empty_run_shell_command_returns_tool_error() {
        assert!(
            agent_command_from_tool_call("run_shell", serde_json::json!({"command": ""})).is_err()
        );
    }
}
