//! Everything that only exists on a desktop.
//!
//! One gate for the lot, rather than an attribute on each declaration. The crate's public paths
//! are unchanged: `lib.rs` re-exports this module's contents, so `vmux_agent::plugin` still
//! resolves from outside and `crate::plugin` still resolves from within.

use bevy::prelude::*;
use vmux_core::agent::{
    PageAgentAttachDefaultRequest, PageAgentAttachRequest, PageAgentSpawnDefaultRequest,
    PageAgentSpawnStackRequest, RestartAgentPty, SpawnAgentInStackRequest,
};
use vmux_terminal::TerminalStackSpawnRequest;

use crate::client::cli::claude::ClaudeStrategy;
use crate::client::cli::codex::CodexStrategy;
use crate::client::cli::vibe::VibeStrategy;
use crate::events::{AgentCommandRequest, AgentQueryRequest, AgentToolCallRequest};
use crate::session::{
    AgentSessionDirty, AgentSessionExited, AgentSessionToEntity, agent_session_dirty_run_condition,
};
use crate::strategy::AgentStrategies;
use vmux_core::browser::{
    BrowserScrollRequest, BrowserSnapshotRequest, BrowserSnapshotResponse, NavAwaitingSnapshot,
};

use self::command::{FocusPaneRequest, ProcessStackSpawnRequest, RenameProfileRequest};

pub use self::attach::{
    attach_acp_agent_to_stack, attach_page_agent_to_stack, page_agent_placeholder_url,
};
pub use self::command::AgentLookups;
pub use self::provider::AgentExecutableOverride;
pub use self::run_terminal::AgentTerminalRegions;
pub use self::spawn::detect_agent_session_process_exit;
pub use vmux_space::cwd::valid_cwd;

pub(crate) use self::follow::on_tidy_action;
pub(crate) use self::run_terminal::agent_terminal_shell;
pub(crate) use self::workspace::{
    PendingAgentChoice, PendingAgentProject, RepositoryNeedsWorktree,
};

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
            crate::plugin::chat::AgentChatPagePlugin,
            crate::plugin::agents::AgentsManagerPlugin,
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
                crate::room::RoomPlugin,
                crate::command_bar::CommandBarPlugin,
                self::attach::AttachPlugin,
                self::attention::AttentionPlugin,
                self::command::CommandPlugin,
                self::follow::FollowPlugin,
                self::page_open::PageOpenPlugin,
                self::provider::ProviderPlugin,
                self::query::QueryPlugin,
                self::self_command::SelfCommandPlugin,
                self::spawn::SpawnPlugin,
                self::workspace::WorkspacePlugin,
            ))
            .init_resource::<AgentSessionToEntity>()
            .init_resource::<AgentTerminalRegions>()
            .init_resource::<AgentSessionDirty>()
            .init_resource::<NavAwaitingSnapshot>()
            .init_resource::<vmux_layout::active_panes::ActivePanes>()
            .init_resource::<vmux_layout::pane::SpawnCounter>()
            .add_message::<AgentCommandRequest>()
            .add_message::<vmux_layout::bookmark::BookmarkOp>()
            .add_message::<vmux_layout::NewTabRequest>()
            .add_message::<vmux_layout::ContributedCommandChosen>()
            .add_message::<FocusPaneRequest>()
            .add_message::<RenameProfileRequest>()
            .add_message::<AgentQueryRequest>()
            .add_message::<ScreenshotRequest>()
            .add_message::<ScreenshotResponse>()
            .add_message::<BrowserSnapshotRequest>()
            .add_message::<BrowserSnapshotResponse>()
            .add_message::<BrowserScrollRequest>()
            .add_message::<vmux_editor::GlobalSearchRequest>()
            .add_message::<vmux_layout::active_panes::ActivatePane>()
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
            .add_message::<vmux_terminal::TerminalReinputRequest>()
            .add_message::<ProcessStackSpawnRequest>()
            .add_message::<RestartAgentPty>()
            .add_message::<vmux_core::agent::SwapStackSession>()
            .add_message::<vmux_core::notify::BellReceived>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_message::<vmux_core::notify::OsNotify>()
            .init_resource::<bevy::ecs::message::Messages<vmux_core::PageOpenRequest>>()
            .init_resource::<bevy::ecs::message::Messages<vmux_layout::OpenBesideRequest>>()
            .init_resource::<bevy::ecs::message::Messages<vmux_layout::CloseStackRequest>>()
            .init_resource::<
                bevy::ecs::message::Messages<vmux_layout::worktree::TabDirectoryObserved>,
            >()
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
    fn blank_cwd_is_accepted() {
        assert_eq!(valid_cwd("").unwrap(), None);
    }

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

pub mod agents;
pub mod chat;

pub mod acp_install;
pub mod acp_registry;
pub mod client;
pub mod command_bar;
pub mod components;
pub mod echo;
pub mod echo_plugin;
pub mod events;
pub mod exec;
pub mod handoff;
pub mod launch;
pub mod managed_mcp;
pub mod mcp;
pub mod providers;
pub mod room;
pub mod run_state;
pub mod run_state_kind;
pub mod session;
pub mod snapshot_updater;
pub mod strategy;
pub mod toast;
pub mod tools;
pub mod url;
pub mod variant;

pub mod systems {
    pub mod approval;
    pub mod surface_errors;
}

pub(crate) mod tidy;

pub use vmux_service::{http, message, stream};

pub use client::acp::AcpSession;
pub use client::cli::strategy::CliAgentStrategy;
pub use components::{AgentApprovalPolicy, AgentMessages, AgentSession, PromptQueue, QueuedPrompt};
pub use events::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
    ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
};
pub use launch::build_agent_launch;
pub use mcp::McpServerConfig;
pub use message::{AssistantBlock, Message};
pub use room::{
    ChatRoom, CollaborativeDocument, CrdtChangeReceived, DocumentKind, MaterializedRoomEvent,
    MemberPresence, MessageDelivery, RoomAgentBinding, RoomEventIdentity, RoomEventIndex,
    RoomIndex, RoomIntent, RoomMember, RoomMessageContent, RoomMetadata, RoomOpCommitted,
    RoomOpReceived, RoomPlugin, RoomProjection, StreamingMessage,
};
pub use run_state::AgentRunState;
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
pub use toast::{AgentToast, ToastLevel};
pub use tools::mcp_tool_defs;
pub use url::{AgentKind, AgentUrl};
pub use variant::AgentVariant;
pub mod attach;
pub mod attention;
pub mod browser_pane;
pub mod command;
pub mod follow;
pub mod page_open;
pub mod provider;
pub mod query;
pub mod run_terminal;
pub mod self_command;
pub mod spawn;
#[cfg(test)]
pub mod test_support;
pub mod workspace;
