use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, JsonValue};

use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolSpawner,
};

pub(super) struct ApplicationToolPlugin;

impl Plugin for ApplicationToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Application))
            .add_systems(Update, dispatch.in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ApplicationTool {
    OpenCommandBar,
    RenameProfile,
    Notify,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenCommandBarArgs {
    mode: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameProfileArgs {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NotifyArgs {
    title: Option<String>,
    body: Option<String>,
}

impl ApplicationTool {
    fn target(self, call: &ToolCall) -> Result<DispatchTarget, String> {
        let command = match self {
            Self::OpenCommandBar => {
                let args: OpenCommandBarArgs = call.parse("open_command_bar")?;
                let id = match args.mode.as_deref().unwrap_or("default") {
                    "default" => "browser_open_command_bar",
                    "commands" => "browser_open_commands",
                    "path" => "browser_open_path_bar",
                    other => return Err(format!("unknown command bar mode: {other}")),
                };
                AgentCommand::InvokeCommand {
                    id: id.to_string(),
                    args: JsonValue::Object(Vec::new()),
                }
            }
            Self::RenameProfile => {
                let args: RenameProfileArgs = call.parse("rename_profile")?;
                if args.name.trim().is_empty() {
                    return Err("rename_profile.name is empty".to_string());
                }
                AgentCommand::RenameProfile { name: args.name }
            }
            Self::Notify => {
                let args: NotifyArgs = call.parse("notify")?;
                AgentCommand::Notify {
                    title: args.title,
                    body: args.body,
                }
            }
        };
        Ok(DispatchTarget::Command(command))
    }
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<ApplicationTool>::from_ron(include_str!("application.ron"));
    tools.spawn_manifest(manifest);
}

fn dispatch(mut commands: Commands, calls: ToolCalls<ApplicationTool>) {
    for (request, call, tool) in calls.iter() {
        call.finish_dispatch(request, &mut commands, tool.target(call));
    }
}
