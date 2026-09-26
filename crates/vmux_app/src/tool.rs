use bevy_app::{App, Plugin};

pub struct ToolPlugin;

impl Plugin for ToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((vmux_command::CommandToolPlugin, vmux_team::TeamToolPlugin));
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
    pub fn mcp_app(
        anchor: Option<vmux_api::protocol::ProcessId>,
        acp_session: bool,
        acp_terminals: bool,
        run_block_timeout: std::time::Duration,
        shell: String,
    ) -> App {
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
        app
    }
}
