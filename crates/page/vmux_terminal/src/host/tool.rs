use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_service::protocol::AgentCommand;

use vmux_mcp::tool::{
    McpToolPlugin, ToolCall, ToolCalls, ToolCommand, ToolDispatchError, ToolDispatchSet,
    ToolRequestSet,
};

pub struct TerminalToolPlugin;

impl Plugin for TerminalToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<TerminalTool>::new(include_str!(
            "tool.ron"
        )))
        .add_systems(Update, parse.in_set(ToolRequestSet))
        .add_systems(Update, dispatch.in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TerminalTool {
    TerminalSend,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalSendArgs {
    text: String,
    terminal: Option<String>,
    enter: Option<bool>,
}

fn parse(mut commands: Commands, calls: ToolCalls<TerminalTool>) {
    for (request, call, _) in calls.matching(TerminalTool::TerminalSend) {
        match call.parse::<TerminalSendArgs>() {
            Ok(args) => {
                commands.entity(request).insert(args);
            }
            Err(message) => {
                commands.entity(request).insert(ToolDispatchError::new(message));
            }
        }
    }
}

fn dispatch(
    mut commands: Commands,
    requests: Query<(Entity, &TerminalSendArgs), (With<ToolCall>, Added<TerminalSendArgs>)>,
) {
    for (entity, args) in &requests {
        let text = if args.enter.unwrap_or(false) {
            format!("{}\r", args.text)
        } else {
            args.text.clone()
        };
        let command = if text.is_empty() {
            Err("terminal_send.text is empty".to_string())
        } else {
            Ok(AgentCommand::TerminalSend {
                text,
                terminal: args.terminal.clone(),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
