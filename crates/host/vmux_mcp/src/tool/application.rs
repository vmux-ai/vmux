use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, JsonValue};

use super::{
    DispatchTarget, NextToolOrder, ParsedToolCall, ToolCalls, ToolDispatchSet, ToolManifest,
    ToolRegistrationSet, ToolRequestSet,
};

pub(super) struct ApplicationToolPlugin;

impl Plugin for ApplicationToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Application))
            .add_systems(Update, parse.in_set(ToolRequestSet))
            .add_systems(
                Update,
                (open_command_bar, rename_profile, notify).in_set(ToolDispatchSet),
            );
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ApplicationTool {
    OpenCommandBar,
    RenameProfile,
    Notify,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenCommandBarArgs {
    mode: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameProfileArgs {
    name: String,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct NotifyArgs {
    title: Option<String>,
    body: Option<String>,
}

fn register(mut commands: Commands, mut next_order: ResMut<NextToolOrder>) {
    ToolManifest::<ApplicationTool>::from_ron(include_str!("application.ron"))
        .spawn(&mut commands, &mut next_order);
}

fn parse(mut commands: Commands, calls: ToolCalls<ApplicationTool>) {
    for (request, call, tool) in calls.iter() {
        match tool {
            ApplicationTool::OpenCommandBar => {
                call.parse_into::<OpenCommandBarArgs>(request, &mut commands)
            }
            ApplicationTool::RenameProfile => {
                call.parse_into::<RenameProfileArgs>(request, &mut commands)
            }
            ApplicationTool::Notify => call.parse_into::<NotifyArgs>(request, &mut commands),
        }
    }
}

fn open_command_bar(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<OpenCommandBarArgs>),
        Added<ParsedToolCall<OpenCommandBarArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let result = match request.args().mode.as_deref().unwrap_or("default") {
            "default" => Ok("browser_open_command_bar"),
            "commands" => Ok("browser_open_commands"),
            "path" => Ok("browser_open_path_bar"),
            other => Err(format!("unknown command bar mode: {other}")),
        };
        let target = result.map(|id| {
            DispatchTarget::Command(AgentCommand::InvokeCommand {
                id: id.to_string(),
                args: JsonValue::Object(Vec::new()),
            })
        });
        request.finish(entity, &mut commands, target);
    }
}

fn rename_profile(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<RenameProfileArgs>),
        Added<ParsedToolCall<RenameProfileArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let name = request.args().name.trim();
        let target = if name.is_empty() {
            Err("rename_profile.name is empty".to_string())
        } else {
            Ok(DispatchTarget::Command(AgentCommand::RenameProfile {
                name: request.args().name.clone(),
            }))
        };
        request.finish(entity, &mut commands, target);
    }
}

fn notify(
    mut commands: Commands,
    requests: Query<(Entity, &ParsedToolCall<NotifyArgs>), Added<ParsedToolCall<NotifyArgs>>>,
) {
    for (entity, request) in &requests {
        request.finish(
            entity,
            &mut commands,
            Ok(DispatchTarget::Command(AgentCommand::Notify {
                title: request.args().title.clone(),
                body: request.args().body.clone(),
            })),
        );
    }
}
