use std::io;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_core::cli::{CliInvocation, CliResult};
use vmux_core::host::manifest::FeaturePlugin;

pub struct VmuxCliPlugin;

impl Plugin for VmuxCliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FeaturePlugin::<crate::Feature>::default(),
            vmux_service::ServiceCliPlugin,
            vmux_tool::ToolCliPlugin,
            vmux_mcp::McpCliPlugin,
        ))
        .add_plugins((
            vmux_command::CommandToolPlugin,
            vmux_team::TeamToolPlugin,
            vmux_browser::BrowserToolPlugin,
            vmux_input::CapturePlugin,
            vmux_chat::host::ChatToolPlugin,
            vmux_editor::FileToolPlugin,
            vmux_knowledge::KnowledgeToolPlugin,
            vmux_vault::VaultToolPlugin,
            vmux_layout::tool::LayoutToolPlugin,
            vmux_bookmark::BookmarkToolPlugin,
            vmux_setting::SettingToolPlugin,
            vmux_space::SpaceToolPlugin,
            vmux_terminal::TerminalToolPlugin,
            vmux_simulator::SimulatorToolPlugin,
        ))
        .add_systems(Update, open_app);
    }
}

fn open_app(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        if invocation.is("app.open") {
            commands.entity(entity).insert(CliResult::from_unit(open()));
        }
    }
}

fn open() -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .arg("-a")
            .arg("Vmux")
            .status()?;
        if status.success() {
            return Ok(());
        }
        Err(io::Error::other(format!(
            "open -a Vmux exited with {status}"
        )))
    }

    #[cfg(not(target_os = "macos"))]
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "launching the Vmux app is not supported on this platform yet",
    ))
}
