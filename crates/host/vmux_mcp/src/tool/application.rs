use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, JsonValue};

use super::{
    DispatchTarget, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet, ToolSpawner,
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

impl OpenCommandBarArgs {
    fn command(self) -> Result<AgentCommand, String> {
        let id = match self.mode.as_deref().unwrap_or("default") {
            "default" => "browser_open_command_bar",
            "commands" => "browser_open_commands",
            "path" => "browser_open_path_bar",
            other => return Err(format!("unknown command bar mode: {other}")),
        };
        Ok(AgentCommand::InvokeCommand {
            id: id.to_string(),
            args: JsonValue::Object(Vec::new()),
        })
    }
}

impl RenameProfileArgs {
    fn command(self) -> Result<AgentCommand, String> {
        if self.name.trim().is_empty() {
            return Err("rename_profile.name is empty".to_string());
        }
        Ok(AgentCommand::RenameProfile { name: self.name })
    }
}

impl From<NotifyArgs> for AgentCommand {
    fn from(args: NotifyArgs) -> Self {
        Self::Notify {
            title: args.title,
            body: args.body,
        }
    }
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<ApplicationTool>::from_ron(include_str!("application.ron"));
    tools.spawn_manifest(manifest);
}

fn dispatch(mut commands: Commands, calls: ToolCalls<ApplicationTool>) {
    for (request, call, tool) in calls.iter() {
        let command = match tool {
            ApplicationTool::OpenCommandBar => call
                .parse::<OpenCommandBarArgs>("open_command_bar")
                .and_then(OpenCommandBarArgs::command),
            ApplicationTool::RenameProfile => call
                .parse::<RenameProfileArgs>("rename_profile")
                .and_then(RenameProfileArgs::command),
            ApplicationTool::Notify => call.parse::<NotifyArgs>("notify").map(AgentCommand::from),
        };
        call.finish_dispatch(request, &mut commands, command.map(DispatchTarget::Command));
    }
}
