use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::AgentCommand;

use super::{
    DispatchTarget, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet, ToolSpawner,
};

pub(super) struct TerminalToolPlugin;

impl Plugin for TerminalToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Terminal))
            .add_systems(Update, dispatch.in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TerminalTool {
    TerminalSend,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalSendArgs {
    text: String,
    terminal: Option<String>,
    enter: Option<bool>,
}

impl TerminalSendArgs {
    fn command(self) -> Result<AgentCommand, String> {
        let text = if self.enter.unwrap_or(false) {
            format!("{}\r", self.text)
        } else {
            self.text
        };
        if text.is_empty() {
            return Err("terminal_send.text is empty".to_string());
        }
        Ok(AgentCommand::TerminalSend {
            text,
            terminal: self.terminal,
        })
    }
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<TerminalTool>::from_ron(include_str!("terminal.ron"));
    tools.spawn_manifest(manifest);
}

fn dispatch(mut commands: Commands, calls: ToolCalls<TerminalTool>) {
    for (request, call, _) in calls.matching(TerminalTool::TerminalSend) {
        let target = call
            .parse::<TerminalSendArgs>("terminal_send")
            .and_then(TerminalSendArgs::command)
            .map(DispatchTarget::Command);
        call.finish_dispatch(request, &mut commands, target);
    }
}
