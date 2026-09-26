use bevy::prelude::*;
use vmux_core::agent::{
    PageAgentAttachDefaultRequest, PageAgentAttachRequest, PageAgentSpawnDefaultRequest,
    PageAgentSpawnStackRequest, RestartAgentPty, SpawnAgentInStackRequest,
};
use vmux_core::browser::{
    BrowserNavigationSnapshotResponse, BrowserScrollRequest, BrowserScrollResponse,
    BrowserSnapshotRequest, BrowserSnapshotResponse,
};
use vmux_terminal::TerminalStackSpawnRequest;

use crate::events::{
    AgentCommandRequest, AgentQueryRequest, AgentToolCallRequest, RecordStartRequest,
    RecordStartResponse, RecordStopRequest, RecordStopResponse, ScreenshotRequest,
    ScreenshotResponse,
};
use crate::runtime::cli::claude::ClaudeStrategy;
use crate::runtime::cli::codex::CodexStrategy;
use crate::runtime::cli::vibe::VibeStrategy;
use crate::session;
use crate::strategy::AgentStrategies;

use super::command::{FocusPaneRequest, ProcessStackSpawnRequest, RenameProfileRequest};
pub struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            AgentSessionPlugin,
            AgentPagesPlugin,
            crate::WorkspaceToolPlugin,
            crate::CaptureToolPlugin,
            crate::runtime::AgentRuntimePlugin,
        ));
    }
}

pub struct AgentPagesPlugin;

impl Plugin for AgentPagesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            super::chat::AgentChatPagePlugin,
            crate::vibe::setup::AgentSetupPlugin,
        ));
    }
}

pub struct AgentSessionPlugin;

impl Plugin for AgentSessionPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .spawn(vmux_core::HostSpawnRoute::host("sessions"));
        app.world_mut()
            .spawn(vmux_core::HostSpawnRoute::host("agent"));
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(VibeStrategy));
        strategies.register_cli(Box::new(ClaudeStrategy));
        strategies.register_cli(Box::new(CodexStrategy));
        app.insert_resource(strategies)
            .add_plugins((
                vmux_layout::LayoutContractPlugin,
                vmux_editor::ContractPlugin,
                vmux_terminal::TerminalContractPlugin,
            ))
            .add_plugins((
                vmux_session::room::RoomPlugin,
                crate::command_bar::CommandBarPlugin,
                super::attach::AttachPlugin,
                super::attention::AttentionPlugin,
                super::command::CommandPlugin,
                super::follow::FollowPlugin,
                super::ingress::AgentIngressPlugin,
                super::page_open::PageOpenPlugin,
                super::provider::ProviderPlugin,
                super::query::AgentQueryPlugin,
                super::self_command::SelfCommandPlugin,
                session::AgentSessionLifecyclePlugin,
                super::snapshot_updater::SnapshotPlugin,
                super::spawn::SpawnPlugin,
                super::workspace::WorkspacePlugin,
            ))
            .add_message::<AgentCommandRequest>()
            .add_message::<FocusPaneRequest>()
            .add_message::<RenameProfileRequest>()
            .add_message::<AgentQueryRequest>()
            .add_message::<ScreenshotRequest>()
            .add_message::<ScreenshotResponse>()
            .add_message::<BrowserSnapshotRequest>()
            .add_message::<BrowserSnapshotResponse>()
            .add_message::<BrowserNavigationSnapshotResponse>()
            .add_message::<BrowserScrollRequest>()
            .add_message::<BrowserScrollResponse>()
            .add_message::<RecordStartRequest>()
            .add_message::<RecordStartResponse>()
            .add_message::<RecordStopRequest>()
            .add_message::<RecordStopResponse>()
            .add_message::<vmux_simulator::SimulatorControlRequest>()
            .add_message::<vmux_simulator::SimulatorControlResponse>()
            .add_message::<vmux_simulator::SimulatorScreenshotRequest>()
            .add_message::<vmux_simulator::SimulatorScreenshotResponse>()
            .add_message::<AgentToolCallRequest>()
            .add_message::<SpawnAgentInStackRequest>()
            .add_message::<PageAgentAttachRequest>()
            .add_message::<PageAgentSpawnStackRequest>()
            .add_message::<PageAgentSpawnDefaultRequest>()
            .add_message::<PageAgentAttachDefaultRequest>()
            .add_message::<TerminalStackSpawnRequest>()
            .add_message::<ProcessStackSpawnRequest>()
            .add_message::<RestartAgentPty>()
            .add_message::<vmux_core::agent::SwapStackSession>()
            .add_message::<vmux_core::notify::BellReceived>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_message::<vmux_core::notify::OsNotify>()
            .init_resource::<bevy::ecs::message::Messages<vmux_core::PageOpenRequest>>()
            .add_systems(Update, super::run_terminal::remember_configured_shell);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::agent::{AgentKind, AgentProviderTargetKind};

    #[test]
    fn agent_plugin_registers_all_three_provider_entries() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
        ));
        app.world_mut().run_schedule(Startup);
        let mut q = app.world_mut().query::<&AgentProviderTargetKind>();
        let ids: std::collections::HashSet<&'static str> =
            q.iter(app.world()).map(|p| p.0.as_url_segment()).collect();
        for id in ["vibe", "claude", "codex"] {
            assert!(ids.contains(id), "missing provider: {id}");
        }
    }

    #[test]
    fn agent_plugin_registers_three_cli_strategies() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
        ));
        let strategies = app.world().resource::<AgentStrategies>();
        assert!(strategies.get_cli(AgentKind::Vibe).is_some());
        assert!(strategies.get_cli(AgentKind::Claude).is_some());
        assert!(strategies.get_cli(AgentKind::Codex).is_some());
    }
}
