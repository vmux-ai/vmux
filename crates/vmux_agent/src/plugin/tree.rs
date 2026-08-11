//! The agent domain's plugin tree.
//!
//! [`AgentPlugin`] is the whole of it; every other module in `plugin/` is one of its slices.

use bevy::prelude::*;
use vmux_core::agent::{
    PageAgentAttachDefaultRequest, PageAgentAttachRequest, PageAgentSpawnDefaultRequest,
    PageAgentSpawnStackRequest, RestartAgentPty, SpawnAgentInStackRequest,
};
use vmux_core::browser::{
    BrowserScrollRequest, BrowserSnapshotRequest, BrowserSnapshotResponse, NavAwaitingSnapshot,
};
use vmux_terminal::TerminalStackSpawnRequest;

use crate::client::cli::claude::ClaudeStrategy;
use crate::client::cli::codex::CodexStrategy;
use crate::client::cli::vibe::VibeStrategy;
use crate::events::{
    AgentCommandRequest, AgentQueryRequest, AgentToolCallRequest, RecordStartRequest,
    RecordStartResponse, RecordStopRequest, RecordStopResponse, ScreenshotRequest,
    ScreenshotResponse,
};
use crate::session::{
    self, AgentSessionDirty, AgentSessionExited, AgentSessionToEntity,
    agent_session_dirty_run_condition,
};
use crate::strategy::AgentStrategies;

use super::command::{FocusPaneRequest, ProcessStackSpawnRequest, RenameProfileRequest};
use super::run_terminal::AgentTerminalRegions;

/// Root plugin for the agent domain, aggregating session lifecycle, the agent pages, and
/// the agent clients (ACP and in-page providers).
pub struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            AgentSessionPlugin,
            AgentPagesPlugin,
            crate::client::AgentClientPlugin,
        ));
    }
}

/// Wires the agent-owned pages: the chat page, the agents manager page, and the setup flow.
pub struct AgentPagesPlugin;

impl Plugin for AgentPagesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            super::chat::AgentChatPagePlugin,
            super::agents::AgentsManagerPlugin,
            crate::vibe::setup::AgentSetupPlugin,
        ));
    }
}

/// Wires the agent domain: CLI agent strategies, session watching, discovery and exit
/// detection, and handling of agent commands, queries, tool calls, screenshots, and recordings.
pub struct AgentSessionPlugin;

impl Plugin for AgentSessionPlugin {
    fn build(&self, app: &mut App) {
        vmux_core::register_host_spawn(app, "agent");
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(VibeStrategy));
        strategies.register_cli(Box::new(ClaudeStrategy));
        strategies.register_cli(Box::new(CodexStrategy));

        app.insert_resource(strategies)
            .add_plugins((
                vmux_layout::LayoutContractPlugin,
                vmux_editor::EditorContractPlugin,
                vmux_terminal::TerminalContractPlugin,
            ))
            .add_plugins((
                crate::room::RoomPlugin,
                crate::command_bar::CommandBarPlugin,
                super::attach::AttachPlugin,
                super::attention::AttentionPlugin,
                super::browser_pane::AgentBrowserPanePlugin,
                super::command::CommandPlugin,
                super::follow::FollowPlugin,
                super::page_open::PageOpenPlugin,
                super::provider::ProviderPlugin,
                super::query::QueryPlugin,
                super::self_command::SelfCommandPlugin,
                super::spawn::SpawnPlugin,
                super::workspace::WorkspacePlugin,
            ))
            .init_resource::<AgentSessionToEntity>()
            .init_resource::<AgentTerminalRegions>()
            .init_resource::<AgentSessionDirty>()
            .init_resource::<NavAwaitingSnapshot>()
            .add_message::<AgentCommandRequest>()
            .add_message::<FocusPaneRequest>()
            .add_message::<RenameProfileRequest>()
            .add_message::<AgentQueryRequest>()
            .add_message::<ScreenshotRequest>()
            .add_message::<ScreenshotResponse>()
            .add_message::<BrowserSnapshotRequest>()
            .add_message::<BrowserSnapshotResponse>()
            .add_message::<BrowserScrollRequest>()
            .add_message::<RecordStartRequest>()
            .add_message::<RecordStartResponse>()
            .add_message::<RecordStopRequest>()
            .add_message::<RecordStopResponse>()
            .add_message::<AgentToolCallRequest>()
            .add_message::<AgentSessionExited>()
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
            .add_systems(Startup, session::start_agent_session_watchers)
            .add_systems(
                Update,
                (
                    session::track_session_id_inserts,
                    session::track_session_id_removals,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    session::mark_dirty_on_fs_change,
                    session::mark_dirty_on_pending_added,
                ),
            )
            .add_systems(
                Update,
                (
                    session::discover_pending_agent_sessions,
                    session::detect_file_end_time_exit,
                    session::clear_agent_session_dirty,
                )
                    .chain()
                    .after(session::mark_dirty_on_fs_change)
                    .after(session::mark_dirty_on_pending_added)
                    .run_if(agent_session_dirty_run_condition),
            )
            .add_systems(
                Update,
                session::format_agent_url.after(session::track_session_id_inserts),
            )
            .add_systems(
                Update,
                (
                    crate::snapshot_updater::update_agents_snapshot,
                    crate::snapshot_updater::update_recent_agents,
                    crate::snapshot_updater::update_agent_sessions_snapshot,
                )
                    .chain()
                    .in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
            );
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
