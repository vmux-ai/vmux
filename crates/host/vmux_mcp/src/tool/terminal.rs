use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::AgentCommand;

use super::{
    DispatchTarget, ParsedToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolRequestSet, ToolSpawner,
};

pub(super) struct TerminalToolPlugin;

impl Plugin for TerminalToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Terminal))
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

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<TerminalTool>::from_ron(include_str!("terminal.ron"));
    tools.spawn_manifest(manifest);
}

fn parse(mut commands: Commands, calls: ToolCalls<TerminalTool>) {
    for (request, call, _) in calls.matching(TerminalTool::TerminalSend) {
        call.parse_into::<TerminalSendArgs>(request, &mut commands);
    }
}

fn dispatch(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<TerminalSendArgs>),
        Added<ParsedToolCall<TerminalSendArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let args = request.args();
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
        request.finish(entity, &mut commands, target);
    }
}
