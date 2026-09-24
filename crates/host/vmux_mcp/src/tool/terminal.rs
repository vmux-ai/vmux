use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::AgentCommand;

use super::{
    DispatchTarget, McpToolPlugin, ToolCall, ToolCalls, ToolDispatchResult, ToolDispatchSet,
    ToolRequestSet,
};

pub(super) struct TerminalToolPlugin;

impl Plugin for TerminalToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<TerminalTool>::new(include_str!(
            "terminal.ron"
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
                commands
                    .entity(request)
                    .insert(ToolDispatchResult(Err(message)));
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
        let target = if text.is_empty() {
            Err("terminal_send.text is empty".to_string())
        } else {
            Ok(DispatchTarget::Command(AgentCommand::TerminalSend {
                text,
                terminal: args.terminal.clone(),
            }))
        };
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}
