use std::io;
use std::time::Duration;

use bevy_app::{App, Plugin};
use vmux_service::protocol::ProcessId;

struct ToolServerPlugin {
    anchor: Option<ProcessId>,
    acp_session: bool,
    acp_terminals: bool,
    run_block_timeout: Duration,
    shell: String,
}

impl Plugin for ToolServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_command::CommandToolPlugin,
            vmux_team::TeamToolPlugin,
            vmux_browser::BrowserToolPlugin,
            vmux_editor::FileToolPlugin,
            vmux_knowledge::KnowledgeToolPlugin,
            vmux_layout::tool::LayoutToolPlugin,
            vmux_layout::bookmark_tool::BookmarkToolPlugin,
            vmux_setting::SettingToolPlugin,
            vmux_space::SpaceToolPlugin,
            vmux_terminal::TerminalToolPlugin,
            vmux_agent::WorkspaceToolPlugin,
            vmux_agent::CaptureToolPlugin,
            vmux_simulator::SimulatorToolPlugin,
        ));
        app.add_plugins(vmux_mcp::protocol::McpPlugin::new(
            self.anchor,
            self.acp_session,
            self.acp_terminals,
            self.run_block_timeout,
            self.shell.clone(),
        ));
    }
}

pub async fn run(
    anchor: Option<String>,
    profile: Option<String>,
    acp_session: bool,
    acp_terminals: bool,
    run_timeout_secs: u64,
    shell: String,
) -> io::Result<()> {
    if let Some(p) = profile
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
    {
        unsafe { std::env::set_var("VMUX_PROFILE", p) };
    }
    let anchor = anchor.and_then(|s| s.parse::<ProcessId>().ok());
    let mut app = App::new();
    app.add_plugins(ToolServerPlugin {
        anchor,
        acp_session,
        acp_terminals,
        run_block_timeout: Duration::from_secs(run_timeout_secs),
        shell,
    });
    vmux_mcp::protocol::run_stdio(app).await
}
