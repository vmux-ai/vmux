use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::{AgentCommand, AgentTerminalSend};

use vmux_tool::{AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin};

pub struct TerminalToolPlugin;

impl Plugin for TerminalToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::new(include_str!("tool.ron")))
            .register_tool::<TerminalSendArgs>("terminal_send")
            .add_systems(Update, dispatch.in_set(ToolDispatchSet));
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalSendArgs {
    text: String,
    terminal: Option<String>,
    enter: Option<bool>,
}

fn dispatch(
    mut commands: Commands,
    requests: Query<(Entity, &TerminalSendArgs), AddedTool<TerminalSendArgs>>,
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
            Ok(AgentCommand::TerminalSend(AgentTerminalSend {
                text,
                terminal: args.terminal.clone(),
            }))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
