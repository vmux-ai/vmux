use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{AgentCommand, JsonValue};

use vmux_mcp::tool::{
    McpToolPlugin, ToolCall, ToolCalls, ToolCommand, ToolDispatchError, ToolDispatchSet,
    ToolRequestSet,
};

pub struct ToolPlugin;

impl Plugin for ToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ApplicationToolPlugin);
        #[cfg(feature = "browser")]
        app.add_plugins(vmux_browser::BrowserToolPlugin);
        #[cfg(feature = "editor")]
        app.add_plugins(vmux_editor::FileToolPlugin);
        #[cfg(feature = "knowledge")]
        app.add_plugins(vmux_knowledge::KnowledgeToolPlugin);
        #[cfg(feature = "layout")]
        app.add_plugins((
            vmux_layout::tool::LayoutToolPlugin,
            vmux_layout::bookmark_tool::BookmarkToolPlugin,
        ));
        #[cfg(feature = "core")]
        app.add_plugins(vmux_setting::SettingToolPlugin);
        #[cfg(feature = "space")]
        app.add_plugins(vmux_space::SpaceToolPlugin);
        #[cfg(feature = "terminal")]
        app.add_plugins(vmux_terminal::TerminalToolPlugin);
        #[cfg(feature = "agent")]
        app.add_plugins((
            vmux_agent::WorkspaceToolPlugin,
            vmux_agent::VisualToolPlugin,
        ));
    }
}

impl ToolPlugin {
    pub fn server(
        anchor: Option<vmux_api::protocol::ProcessId>,
        acp_session: bool,
        acp_terminals: bool,
        run_block_timeout: std::time::Duration,
        shell: String,
    ) -> vmux_mcp::protocol::McpServer {
        let mut app = App::new();
        app.add_plugins((
            Self,
            vmux_mcp::protocol::McpPlugin::new(
                anchor,
                acp_session,
                acp_terminals,
                run_block_timeout,
                shell,
            ),
        ));
        vmux_mcp::protocol::McpServer::from(app)
    }
}

pub struct ApplicationToolPlugin;

impl Plugin for ApplicationToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<ApplicationTool>::new(include_str!(
            "tool.ron"
        )))
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

fn parse(mut commands: Commands, calls: ToolCalls<ApplicationTool>) {
    for (request, call, tool) in calls.iter() {
        let parsed = match tool {
            ApplicationTool::OpenCommandBar => call.parse::<OpenCommandBarArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            ApplicationTool::RenameProfile => call.parse::<RenameProfileArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            ApplicationTool::Notify => call.parse::<NotifyArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
        };
        if let Err(message) = parsed {
            commands.entity(request).insert(ToolDispatchError::new(message));
        }
    }
}

fn open_command_bar(
    mut commands: Commands,
    requests: Query<(Entity, &OpenCommandBarArgs), (With<ToolCall>, Added<OpenCommandBarArgs>)>,
) {
    for (entity, args) in &requests {
        let result = match args.mode.as_deref().unwrap_or("default") {
            "default" => Ok("browser_open_command_bar"),
            "commands" => Ok("browser_open_commands"),
            "path" => Ok("browser_open_path_bar"),
            other => Err(format!("unknown command bar mode: {other}")),
        };
        let command = result.map(|id| AgentCommand::InvokeCommand {
            id: id.to_string(),
            args: JsonValue::Object(Vec::new()),
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn rename_profile(
    mut commands: Commands,
    requests: Query<(Entity, &RenameProfileArgs), (With<ToolCall>, Added<RenameProfileArgs>)>,
) {
    for (entity, args) in &requests {
        let name = args.name.trim();
        let command = if name.is_empty() {
            Err("rename_profile.name is empty".to_string())
        } else {
            Ok(AgentCommand::RenameProfile {
                name: args.name.clone(),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn notify(
    mut commands: Commands,
    requests: Query<(Entity, &NotifyArgs), (With<ToolCall>, Added<NotifyArgs>)>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::Notify {
                title: args.title.clone(),
                body: args.body.clone(),
            })));
    }
}
