use serde::Serialize;
use serde_json::{Value, json};
use vmux_service::protocol::{AgentCommand, AgentShellMode};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
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
        },
        ToolDefinition {
            name: "new_tab".to_string(),
            description: "Create a new Vmux tab and open the command bar.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
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
        },
        ToolDefinition {
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
        },
    ]
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
        "new_tab" => Ok(AgentCommand::AppCommand {
            id: "tab_new".to_string(),
        }),
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
        other => Err(format!("unknown tool: {other}")),
    }
}

fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_tools_exposes_mvp_tools() {
        let names: Vec<_> = tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect();

        assert_eq!(
            names,
            vec![
                "open_command_bar",
                "new_tab",
                "new_terminal_tab",
                "run_shell"
            ]
        );
    }

    #[test]
    fn empty_run_shell_command_returns_tool_error() {
        assert!(
            agent_command_from_tool_call("run_shell", serde_json::json!({"command": ""})).is_err()
        );
    }
}
