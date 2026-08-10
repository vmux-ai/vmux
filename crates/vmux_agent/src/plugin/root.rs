//! The agent domain's plugin tree.
//!
//! [`AgentPlugin`] is the whole of it; everything below is one of its slices, split by what an
//! agent does rather than by which kind of item implements it. The three plugins here are the
//! table of contents.

mod attach;
mod attention;
mod browser_pane;
mod command;
mod follow;
mod page_open;
mod provider;
mod query;
mod run_terminal;
mod self_command;
mod spawn;
#[cfg(test)]
mod test_support;
mod workspace;

use bevy::prelude::*;
use vmux_command::WriteAppCommands;
use vmux_core::PageOpenSet;
use vmux_core::agent::{
    PageAgentAttachDefaultRequest, PageAgentAttachRequest, PageAgentSpawnDefaultRequest,
    PageAgentSpawnStackRequest, RestartAgentPty, SpawnAgentInStackRequest,
};
use vmux_terminal::{ServiceMessageSet, TerminalStackSpawnRequest};

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
use vmux_core::browser::{
    BrowserScrollRequest, BrowserSnapshotRequest, BrowserSnapshotResponse, NavAwaitingSnapshot,
};

use self::attach::handle_resume_in_acp;
use self::attention::{
    agent_bell_to_attention, clear_agent_done, handle_agent_turn_ended, mark_agent_done,
};
use self::command::{
    FocusPaneRequest, ProcessStackSpawnRequest, RenameProfileRequest, handle_agent_commands,
    handle_agent_tool_calls, handle_focus_pane_requests, handle_rename_profile_requests,
};
use self::follow::{
    handle_agent_file_search, handle_agent_file_touch, tidy_acp_on_idle, tidy_on_agent_attention,
    tidy_page_on_idle,
};
use self::page_open::{
    handle_agent_page_open, handle_swap_stack_session, prepare_agent_tab_worktrees,
};
use self::provider::{detect_agent_provider_availability, spawn_builtin_agent_providers};
use self::query::{
    forward_layout_apply_responses, forward_layout_snapshot_responses,
    forward_record_start_responses, forward_record_stop_responses, forward_screenshot_responses,
    forward_snapshot_responses, handle_agent_queries,
};
use self::self_command::handle_agent_self_commands;
use self::spawn::{
    handle_restart_agent_pty, handle_spawn_agent_requests, respond_page_agent_attach,
    respond_page_agent_attach_default, respond_page_agent_spawn_default,
    respond_page_agent_spawn_stack, respond_process_stack_spawn,
};
use self::workspace::{
    drain_workspace_picker_tasks, handle_agent_choice_selected, send_pending_agent_continuations,
};

pub use self::attach::{
    attach_acp_agent_to_stack, attach_page_agent_to_stack, page_agent_placeholder_url,
};
pub use self::command::AgentLookups;
pub use self::provider::AgentExecutableOverride;
pub use self::run_terminal::AgentTerminalRegions;
pub use self::spawn::detect_agent_session_process_exit;
pub use vmux_space::cwd::valid_cwd;

pub(crate) use self::command::forward_history_open_intent;
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
            .add_plugins((crate::room::RoomPlugin, crate::command_bar::CommandBarPlugin))
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
            .add_observer(handle_agent_choice_selected)
            .init_resource::<bevy::ecs::message::Messages<vmux_core::PageOpenRequest>>()
            .init_resource::<bevy::ecs::message::Messages<vmux_layout::OpenBesideRequest>>()
            .init_resource::<bevy::ecs::message::Messages<vmux_layout::CloseStackRequest>>()
            .init_resource::<
                bevy::ecs::message::Messages<vmux_layout::worktree::TabDirectoryObserved>,
            >()
            .add_systems(
                Update,
                (
                    agent_bell_to_attention,
                    handle_agent_turn_ended,
                    tidy_on_agent_attention,
                    mark_agent_done,
                    clear_agent_done,
                )
                    .chain()
                    .after(vmux_layout::stack::ComputeFocusSet),
            )
            .add_systems(
                Update,
                (tidy_acp_on_idle, tidy_page_on_idle).after(vmux_layout::stack::ComputeFocusSet),
            )
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
                    forward_history_open_intent,
                    handle_agent_tool_calls,
                    handle_resume_in_acp,
                    handle_agent_commands,
                    handle_agent_file_touch.before(vmux_layout::worktree::TabDirectoryRebindSet),
                    handle_agent_file_search,
                )
                    .chain()
                    .in_set(WriteAppCommands)
                    .after(ServiceMessageSet),
            )
            .add_systems(
                Update,
                (
                    handle_agent_self_commands
                        .after(vmux_layout::worktree::TabDirectoryRebindSet)
                        .before(vmux_terminal::plugin::respond_terminal_stack_spawn),
                    drain_workspace_picker_tasks,
                    send_pending_agent_continuations,
                    handle_agent_queries,
                    detect_agent_session_process_exit,
                )
                    .chain()
                    .in_set(WriteAppCommands)
                    .after(ServiceMessageSet),
            )
            .add_systems(
                Update,
                (
                    forward_layout_apply_responses,
                    forward_layout_snapshot_responses,
                    forward_screenshot_responses,
                    forward_snapshot_responses,
                    forward_record_start_responses,
                    forward_record_stop_responses,
                ),
            )
            .add_systems(
                Update,
                (
                    handle_spawn_agent_requests,
                    handle_swap_stack_session.before(handle_spawn_agent_requests),
                    handle_focus_pane_requests.after(handle_agent_commands),
                    handle_rename_profile_requests.after(handle_agent_commands),
                    respond_process_stack_spawn.after(handle_agent_commands),
                    prepare_agent_tab_worktrees
                        .in_set(PageOpenSet::HandleKnownPages)
                        .before(handle_agent_page_open),
                    handle_agent_page_open.in_set(PageOpenSet::HandleKnownPages),
                    handle_restart_agent_pty.before(ServiceMessageSet),
                    respond_page_agent_attach,
                    respond_page_agent_spawn_stack,
                    respond_page_agent_spawn_default,
                    respond_page_agent_attach_default,
                ),
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
            )
            .add_systems(
                Startup,
                (
                    spawn_builtin_agent_providers,
                    detect_agent_provider_availability,
                )
                    .chain(),
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

    #[test]
    fn agent_run_spawns_terminal_before_next_agent_command_frame() {
        let source = include_str!("root.rs");
        let non_test_source = source
            .split("#[cfg(test)]\nmod tests {")
            .next()
            .expect("non-test source");
        let start = non_test_source
            .find("handle_agent_self_commands")
            .expect("handle_agent_self_commands registered");
        assert!(
            non_test_source[start..]
                .contains(".before(vmux_terminal::plugin::respond_terminal_stack_spawn)"),
            "run terminal spawn requests must materialize before the next agent command frame"
        );
    }

    #[test]
    fn agent_restart_runs_before_terminal_service_messages() {
        let source = include_str!("root.rs");
        let non_test_source = source
            .split("#[cfg(test)]\nmod tests {")
            .next()
            .expect("non-test source");

        assert!(
            non_test_source.contains("handle_restart_agent_pty.before(ServiceMessageSet)"),
            "restart state commands must apply before terminal input flush"
        );
    }
}
