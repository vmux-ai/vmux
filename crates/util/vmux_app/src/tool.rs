use bevy_app::{App, Plugin};

pub struct VmuxToolPlugin;

impl Plugin for VmuxToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_service::ServiceCliPlugin,
            vmux_tool::ToolCliPlugin,
            vmux_mcp::McpCliPlugin,
        ))
        .add_plugins((
            vmux_command::CommandToolPlugin,
            vmux_team::TeamToolPlugin,
            vmux_browser::BrowserToolPlugin,
            vmux_input::CapturePlugin,
            vmux_session::host::ChatToolPlugin,
            vmux_editor::FileToolPlugin,
            vmux_knowledge::KnowledgeToolPlugin,
            vmux_vault::VaultToolPlugin,
            vmux_layout::tool::LayoutToolPlugin,
            vmux_bookmark::BookmarkToolPlugin,
            vmux_setting::SettingToolPlugin,
            vmux_space::SpaceToolPlugin,
            vmux_terminal::TerminalToolPlugin,
            vmux_simulator::SimulatorToolPlugin,
        ));
    }
}
