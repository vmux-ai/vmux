use std::io;
use std::time::Duration;

use bevy_app::{App, Plugin};
use clap::Args;
use vmux_service::protocol::ProcessId;

#[derive(Clone, Debug, Args)]
pub struct McpArgs {
    #[arg(long)]
    anchor: Option<String>,
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    acp_session: bool,
    #[arg(long)]
    acp_terminals: bool,
    #[arg(long, default_value_t = 50)]
    run_timeout_secs: u64,
    #[arg(long, default_value_t = String::new())]
    shell: String,
}

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

pub async fn run(args: McpArgs) -> io::Result<()> {
    if let Some(profile) = args
        .profile
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
    {
        unsafe { std::env::set_var("VMUX_PROFILE", profile) };
    }
    let anchor = args
        .anchor
        .and_then(|value| value.parse::<ProcessId>().ok());
    let mut app = App::new();
    app.add_plugins(ToolServerPlugin {
        anchor,
        acp_session: args.acp_session,
        acp_terminals: args.acp_terminals,
        run_block_timeout: Duration::from_secs(args.run_timeout_secs),
        shell: args.shell,
    });
    vmux_mcp::protocol::run_stdio(app).await
}
