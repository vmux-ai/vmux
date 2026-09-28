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

use crate::event::{
    AgentQueryRequest, AgentRequestInput, AgentToolCallRequest, RecordStartRequest,
    RecordStartResponse, RecordStopRequest, RecordStopResponse, ScreenshotRequest,
    ScreenshotResponse,
};
use crate::session;

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
            vmux_chat::ChatPlugin,
            super::model::ChatModelPlugin,
            super::prompt::ChatPromptPlugin,
            super::resume::ChatResumePlugin,
            super::transcript::ChatTranscriptPlugin,
            crate::setup::AgentSetupPlugin,
        ));
    }
}

pub struct AgentSessionPlugin;

impl Plugin for AgentSessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, spawn_agent_store_validator);
        app.add_systems(
            Startup,
            (spawn_agent_strategies, register_agent_session_route).chain(),
        );
        app.add_plugins((
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
        .add_message::<AgentRequestInput>()
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
        .add_message::<vmux_simulator::SimulatorTapRequest>()
        .add_message::<vmux_simulator::SimulatorSwipeRequest>()
        .add_message::<vmux_simulator::SimulatorTypeTextRequest>()
        .add_message::<vmux_simulator::SimulatorKeyPressRequest>()
        .add_message::<vmux_simulator::SimulatorButtonPressRequest>()
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
        .add_message::<RestartAgentPty>()
        .add_message::<vmux_core::agent::SwapStackSession>()
        .add_message::<vmux_core::notify::BellReceived>()
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_message::<vmux_core::notify::OsNotify>()
        .init_resource::<bevy::ecs::message::Messages<vmux_core::PageOpenRequest>>()
        .add_systems(Update, super::run_terminal::remember_configured_shell);
    }
}

fn spawn_agent_store_validator(mut commands: Commands) {
    commands.spawn((
        Name::new("Agent workspace-store validator"),
        vmux_core::host::persistence::WorkspaceStoreValidator {
            name: "agent URL",
            rejects: crate::AgentUrl::rejects_persisted_store,
        },
    ));
}

fn spawn_agent_strategies(mut commands: Commands) {
    commands.spawn((Name::new("Vibe CLI strategy"), super::cli::VIBE));
    commands.spawn((Name::new("Claude CLI strategy"), super::cli::CLAUDE));
    commands.spawn((Name::new("Codex CLI strategy"), super::cli::CODEX));
}

fn register_agent_session_route(mut commands: Commands) {
    commands.spawn(vmux_core::HostSpawnRoute::subtree("vmux://sessions/"));
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
        app.world_mut().run_schedule(Startup);
        let kinds = app
            .world_mut()
            .query::<&crate::CliStrategy>()
            .iter(app.world())
            .map(|strategy| strategy.kind)
            .collect::<std::collections::HashSet<_>>();
        assert!(kinds.contains(&AgentKind::Vibe));
        assert!(kinds.contains(&AgentKind::Claude));
        assert!(kinds.contains(&AgentKind::Codex));
    }
}
