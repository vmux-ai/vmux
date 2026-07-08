use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::{CefKeyboardTarget, WebviewExtendStandardMaterial};
use vmux_command::{AppCommand, WriteAppCommands};
use vmux_core::agent::{
    AgentKind, AgentProviderTargetKind, PageAgentAttachDefaultRequest, PageAgentAttachRequest,
    PageAgentSpawnDefaultRequest, PageAgentSpawnStackRequest, RestartAgentPty,
    SpawnAgentInStackRequest,
};
use vmux_core::{
    LastActivatedAt, PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask, Ready,
};
use vmux_layout::event::TERMINAL_PAGE_URL;
use vmux_layout::{
    pane::{ForcePaneClose, Pane, PaneSplit},
    stack::FocusedStack,
};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{
    AgentCommand as ServiceAgentCommand, AgentCommandResult, AgentQuery, AgentQueryResult,
    AgentRequestId, AgentShellMode, ClientMessage, ProcessId,
};
use vmux_setting::AppSettings;
use vmux_space::ActiveSpace;
use vmux_terminal::launch::TerminalLaunch;
use vmux_terminal::{
    AwaitingProcessCreated, ProcessExited, ServiceMessageSet, Terminal, TerminalGridSize,
    TerminalStackSpawnRequest, new_terminal_bundle_with_cwd,
};

use crate::AgentVariant;
use crate::client::cli::claude::ClaudeStrategy;
use crate::client::cli::codex::CodexStrategy;
use crate::client::cli::vibe::VibeStrategy;
use crate::events::{
    AgentCommandRequest, AgentQueryRequest, AgentToolCallRequest, BrowserScrollRequest,
    BrowserSnapshotRequest, BrowserSnapshotResponse, CommandOrigin, NavAwaitingSnapshot,
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
    ScreenshotImage, ScreenshotRequest, ScreenshotResponse, snapshot_response_to_query_result,
};
use crate::session::{
    self, AgentSession, AgentSessionDirty, AgentSessionExited, AgentSessionToEntity,
    PendingAgentSession, SessionId, agent_session_dirty_run_condition,
};
use crate::strategy::AgentStrategies;

pub use vmux_space::cwd::{default_space_dir, space_dir, valid_cwd};

const BUILTIN_AGENT_PROVIDERS: &[AgentKind] =
    &[AgentKind::Vibe, AgentKind::Claude, AgentKind::Codex];

/// Per-[`AgentKind`] override for CLI executable resolution: `true` forces present, `false` forces
/// missing, absent falls back to a real `PATH` lookup. Lets tests drive the spawn/setup-page flow
/// without depending on which CLIs are installed on the host.
#[derive(Resource, Clone, Default)]
pub struct AgentExecutableOverride(pub std::collections::HashMap<AgentKind, bool>);

#[derive(Resource, Default)]
pub struct AgentTerminalRegions {
    pub run_terminals: std::collections::HashMap<ProcessId, ProcessId>,
    pub run_panes: std::collections::HashMap<ProcessId, Entity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RunTerminalCandidate {
    pid: ProcessId,
    stack: Entity,
    pane: Entity,
    pane_spawn_seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RunTerminalBucketPaneCandidate {
    pane: Entity,
    pane_spawn_seq: u64,
}

fn choose_reusable_run_terminal(
    anchor: ProcessId,
    agent_pane: Entity,
    regions: &AgentTerminalRegions,
    candidates: &[RunTerminalCandidate],
) -> Option<RunTerminalCandidate> {
    if let Some(pid) = regions.run_terminals.get(&anchor)
        && let Some(candidate) = candidates.iter().find(|c| c.pid == *pid)
    {
        return Some(*candidate);
    }
    if let Some(pane) = regions.run_panes.get(&anchor)
        && let Some(candidate) = candidates
            .iter()
            .filter(|c| c.pane == *pane)
            .max_by_key(|c| c.pane_spawn_seq)
    {
        return Some(*candidate);
    }
    candidates
        .iter()
        .filter(|c| c.pane != agent_pane)
        .max_by_key(|c| c.pane_spawn_seq)
        .copied()
}

fn choose_run_terminal_bucket_pane(
    anchor: ProcessId,
    agent_pane: Entity,
    regions: &AgentTerminalRegions,
    candidates: &[RunTerminalCandidate],
) -> Option<Entity> {
    choose_reusable_run_terminal(anchor, agent_pane, regions, candidates)
        .map(|c| c.pane)
        .or_else(|| {
            regions
                .run_panes
                .get(&anchor)
                .copied()
                .filter(|pane| *pane != agent_pane)
        })
}

fn resolve_agent_executable(
    kind: AgentKind,
    override_: Option<&AgentExecutableOverride>,
) -> Option<PathBuf> {
    if let Some(forced) = override_.and_then(|o| o.0.get(&kind).copied()) {
        return forced.then(|| PathBuf::from(kind.executable()));
    }
    crate::exec::find_executable(kind.executable())
}

fn spawn_builtin_agent_providers(mut commands: Commands) {
    for kind in BUILTIN_AGENT_PROVIDERS {
        commands.spawn((
            AgentProviderTargetKind(*kind),
            Name::new(kind.display_name()),
        ));
    }
}

fn detect_agent_provider_availability(
    mut commands: Commands,
    q: Query<(Entity, &AgentProviderTargetKind), Without<Ready>>,
) {
    for (entity, kind) in &q {
        if crate::exec::find_executable(kind.0.executable()).is_some() {
            commands.entity(entity).insert(Ready);
        }
    }
}

/// Wires the agent domain: CLI agent strategies, session watching, discovery and exit
/// detection, and handling of agent commands, queries, tool calls, screenshots, and recordings.
pub struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        vmux_core::register_host_spawn(app, "agent");
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(VibeStrategy));
        strategies.register_cli(Box::new(ClaudeStrategy));
        strategies.register_cli(Box::new(CodexStrategy));

        app.insert_resource(strategies)
            .init_resource::<AgentSessionToEntity>()
            .init_resource::<AgentTerminalRegions>()
            .init_resource::<AgentSessionDirty>()
            .init_resource::<NavAwaitingSnapshot>()
            .init_resource::<vmux_layout::pane::SpawnCounter>()
            .add_message::<AgentCommandRequest>()
            .add_message::<FocusPaneRequest>()
            .add_message::<RenameProfileRequest>()
            .add_message::<AgentQueryRequest>()
            .add_message::<ScreenshotRequest>()
            .add_message::<ScreenshotResponse>()
            .add_message::<BrowserSnapshotRequest>()
            .add_message::<BrowserSnapshotResponse>()
            .add_message::<BrowserScrollRequest>()
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
            .add_message::<ProcessStackSpawnRequest>()
            .add_message::<RestartAgentPty>()
            .add_message::<vmux_core::notify::BellReceived>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_message::<vmux_core::notify::OsNotify>()
            .init_resource::<bevy::ecs::message::Messages<vmux_layout::OpenBesideRequest>>()
            .init_resource::<bevy::ecs::message::Messages<vmux_layout::CloseStackRequest>>()
            .add_systems(
                Update,
                (
                    agent_bell_to_attention,
                    tidy_on_agent_attention,
                    mark_agent_done,
                    clear_agent_done,
                )
                    .chain()
                    .after(vmux_layout::stack::ComputeFocusSet),
            )
            .add_systems(
                Update,
                tidy_acp_on_idle.after(vmux_layout::stack::ComputeFocusSet),
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
                    handle_agent_commands,
                    handle_agent_self_commands
                        .before(vmux_terminal::plugin::respond_terminal_stack_spawn),
                    handle_agent_file_touch,
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
                    handle_focus_pane_requests.after(handle_agent_commands),
                    handle_rename_profile_requests.after(handle_agent_commands),
                    respond_process_stack_spawn.after(handle_agent_commands),
                    handle_agent_page_open.in_set(PageOpenSet::HandleKnownPages),
                    handle_restart_agent_pty,
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
                    crate::snapshot_updater::update_agent_sessions_snapshot,
                )
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

pub use crate::build_agent_launch;

pub fn attach_page_agent_to_stack(
    stack: Entity,
    provider: &str,
    model: &str,
    sid: &str,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    idx: &crate::client::page::strategy_index::PageStrategyIndex,
    kind_q: &Query<&crate::client::page::strategy_components::StrategyKind>,
) -> Option<()> {
    let entity = idx.get_by_strs(provider, model)?;
    let kind = kind_q.get(entity).ok()?.0;
    let url = format!("{}{sid}", crate::url::page_url_prefix(provider, model));
    commands.entity(stack).insert(PageMetadata {
        url: url.clone(),
        title: format!("{provider}/{model}"),
        bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
        ..default()
    });
    commands.entity(stack).insert((
        crate::components::AgentSession {
            kind,
            variant: AgentVariant::Page,
            sid: sid.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
        },
        crate::AgentMessages::default(),
        crate::AgentApprovalPolicy::default(),
        crate::AgentRunState::default(),
        vmux_core::team::Profile::agent(kind),
        vmux_core::team::Agent {
            sid: sid.to_string(),
            kind: Some(kind),
        },
    ));
    let url = format!("vmux://agent/{provider}");
    commands.spawn((
        vmux_layout::Browser::new(meshes, webview_mt, &url),
        ChildOf(stack),
    ));
    Some(())
}

#[allow(clippy::too_many_arguments)]
pub fn attach_acp_agent_to_stack(
    stack: Entity,
    agent_id: &str,
    name: &str,
    sid: &str,
    cwd: &std::path::Path,
    icon: Option<&str>,
    resume: Option<&str>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    // A resume carries the agent-assigned session id in the url; a fresh open is bare and gets
    // redirected to `vmux://agent/<id>/<acp-session-id>` once the agent returns its id.
    let url = match resume {
        Some(acp_sid) => format!("vmux://agent/{agent_id}/{acp_sid}"),
        None => format!("vmux://agent/{agent_id}"),
    };
    commands.entity(stack).insert(PageMetadata {
        url: url.clone(),
        title: name.to_string(),
        bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
        icon: vmux_core::PageIcon::favicon(icon.unwrap_or("")),
    });
    let anchor = vmux_service::protocol::ProcessId::new();
    commands.entity(stack).insert((
        crate::client::acp::AcpSession {
            agent_id: agent_id.to_string(),
            sid: sid.to_string(),
            cwd: cwd.to_path_buf(),
            anchor,
            resume: resume.map(str::to_string),
        },
        crate::AgentMessages::default(),
        crate::AgentApprovalPolicy::default(),
        crate::AgentRunState::default(),
        vmux_core::team::Profile::registry(name, agent_id),
        vmux_core::team::Agent {
            sid: sid.to_string(),
            kind: None,
        },
        vmux_core::AgentWorkingDir(cwd.to_string_lossy().to_string()),
    ));
    // The webview carries the anchor `ProcessId`, so vmux_mcp tool calls resolve to this pane.
    commands.spawn((
        vmux_layout::Browser::new(meshes, webview_mt, &url),
        ChildOf(stack),
        anchor,
    ));
}

/// The registry icon URL for an ACP agent id, if the catalog is loaded and lists it.
fn acp_icon_for_id(catalog: Option<&crate::client::acp::AcpCatalog>, id: &str) -> Option<String> {
    catalog?
        .agents
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.icon.clone())
}

#[allow(dead_code)]
pub fn page_agent_placeholder_url(provider: &str, model: &str, sid: &str) -> String {
    let html = format!(
        "<!doctype html><html><head><meta charset='utf-8'><title>Page Agent</title><style>html,body{{height:100%;margin:0;background:#0c0c10;color:#bbb;font-family:-apple-system,BlinkMacSystemFont,sans-serif;display:flex;align-items:center;justify-content:center}}div{{text-align:center;padding:2rem}}h1{{margin:0 0 0.5rem;font-weight:600;color:#eee}}code{{background:#1a1a22;padding:0.15rem 0.4rem;border-radius:4px;color:#e0a050}}</style></head><body><div><h1>Page Agent</h1><p><code>{provider}</code> / <code>{model}</code></p><p>Session <code>{sid}</code></p><p style='opacity:0.6;margin-top:1rem'>Native chat UI ships in step 4 of the Page agent design.</p></div></body></html>"
    );
    let mut encoded = String::with_capacity(html.len() * 3);
    for byte in html.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    format!("data:text/html;charset=utf-8,{encoded}")
}

#[derive(bevy::ecs::system::SystemParam)]
struct SettingsParams<'w> {
    settings: ResMut<'w, AppSettings>,
    writes: MessageWriter<'w, vmux_setting::SettingsWriteRequest>,
}

#[derive(Message, Clone)]
struct ProcessStackSpawnRequest {
    pane: Entity,
    command: String,
    args: Vec<String>,
    cwd: PathBuf,
    env: Vec<(String, String)>,
    activate: bool,
}

#[derive(Message, Clone)]
struct FocusPaneRequest {
    pane: String,
}

fn handle_focus_pane_requests(
    mut reader: MessageReader<FocusPaneRequest>,
    child_of_q: Query<&ChildOf>,
    mut commands: Commands,
) {
    for req in reader.read() {
        let Ok((_, bits)) = vmux_layout::protocol::parse_id(&req.pane) else {
            continue;
        };
        vmux_core::focus_pane_entity(Entity::from_bits(bits), &mut commands, &child_of_q);
    }
}

#[derive(Message, Clone)]
struct RenameProfileRequest {
    name: String,
}

fn handle_rename_profile_requests(
    mut reader: MessageReader<RenameProfileRequest>,
    active_space: Option<ResMut<ActiveSpace>>,
) {
    let Some(mut active) = active_space else {
        return;
    };
    for req in reader.read() {
        let name = req.name.trim();
        if name.is_empty() {
            continue;
        }
        match vmux_core::profile::set_display_name(name) {
            Ok(()) => active.record.profile = name.to_string(),
            Err(error) => warn!("rename_profile: failed to persist display name: {error}"),
        }
    }
}

fn origin_is_agent(origin: &CommandOrigin) -> bool {
    matches!(origin, CommandOrigin::Agent { .. })
}

fn requested_focus_for_origin(origin: &CommandOrigin, requested: bool) -> bool {
    requested && !origin_is_agent(origin)
}

fn focused_id(kind: vmux_layout::protocol::NodeKind, entity: Option<Entity>) -> Option<String> {
    entity.map(|entity| vmux_layout::protocol::format_id(kind, entity.to_bits()))
}

fn preserve_current_focus_in_layout_snapshot(
    snapshot: &mut vmux_service::protocol::layout::LayoutSnapshot,
    focus: &FocusedStack,
) {
    snapshot.focused = vmux_service::protocol::layout::Focus {
        tab: focused_id(vmux_layout::protocol::NodeKind::Tab, focus.tab),
        pane: focused_id(vmux_layout::protocol::NodeKind::Pane, focus.pane),
        stack: focused_id(vmux_layout::protocol::NodeKind::Stack, focus.stack),
    };
    if let Some(tab) = snapshot.focused.tab.as_deref() {
        for item in &mut snapshot.tabs {
            item.is_active = item.id.as_deref() == Some(tab);
        }
    }
}

fn agent_may_dispatch_app_command(command: &AppCommand) -> bool {
    !matches!(
        command,
        AppCommand::Layout(_)
            | AppCommand::Browser(vmux_command::BrowserCommand::Open(_))
            | AppCommand::Browser(vmux_command::BrowserCommand::Bar(_))
            | AppCommand::Service(vmux_command::ServiceCommand::Open)
            | AppCommand::Terminal(vmux_command::TerminalCommand::Next)
            | AppCommand::Terminal(vmux_command::TerminalCommand::Previous)
    )
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct AgentLookups<'w> {
    pub pid_to_entity: Option<Res<'w, vmux_terminal::pid::PidToEntity>>,
    pub agent_to_entity: Option<Res<'w, crate::session::AgentSessionToEntity>>,
    pub active_space: Option<Res<'w, ActiveSpace>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct AgentSpaceWriters<'w, 's> {
    layout_apply: MessageWriter<'w, vmux_layout::reconcile::LayoutApplyRequest>,
    space_command: MessageWriter<'w, vmux_space::SpaceCommandRequest>,
    focus_pane: MessageWriter<'w, FocusPaneRequest>,
    rename_profile: MessageWriter<'w, RenameProfileRequest>,
    issued: MessageWriter<'w, vmux_command::CommandIssued>,
    attention: MessageWriter<'w, vmux_core::notify::AgentAttention>,
    agents: Query<
        'w,
        's,
        (
            Entity,
            &'static vmux_core::team::Agent,
            Option<&'static vmux_service::protocol::ProcessId>,
        ),
    >,
    user: Query<'w, 's, Entity, With<vmux_core::team::User>>,
    browse: AgentBrowserResolve<'w, 's>,
    open_beside: MessageWriter<'w, vmux_layout::OpenBesideRequest>,
}

fn handle_agent_tool_calls(
    mut reader: MessageReader<AgentToolCallRequest>,
    mut command_writer: MessageWriter<AgentCommandRequest>,
    mut query_writer: MessageWriter<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
) {
    for req in reader.read() {
        let args: serde_json::Value =
            serde_json::from_str(&req.args_json).unwrap_or_else(|_| serde_json::json!({}));
        match vmux_mcp::tools::dispatch_from_tool_call(&req.name, args) {
            Ok(vmux_mcp::tools::DispatchTarget::Command(command)) => {
                command_writer.write(AgentCommandRequest {
                    request_id: req.request_id,
                    origin: CommandOrigin::Agent {
                        sid: Some(req.sid.clone()),
                        anchor: None,
                    },
                    command,
                });
            }
            Ok(vmux_mcp::tools::DispatchTarget::Query(query)) => {
                query_writer.write(AgentQueryRequest {
                    request_id: req.request_id,
                    query,
                });
            }
            Err(message) => {
                if let Some(service) = service.as_ref() {
                    service.0.send(ClientMessage::AgentToolResult {
                        request_id: req.request_id,
                        content: message,
                        is_error: true,
                    });
                }
            }
        }
    }
}

fn agent_bell_to_attention(
    mut reader: MessageReader<vmux_core::notify::BellReceived>,
    mut attention: MessageWriter<vmux_core::notify::AgentAttention>,
    agents: Query<(Entity, &vmux_service::protocol::ProcessId), With<vmux_core::team::Agent>>,
) {
    for ev in reader.read() {
        if let Some((entity, _)) = agents.iter().find(|(_, pid)| **pid == ev.process_id) {
            attention.write(vmux_core::notify::AgentAttention {
                entity,
                title: None,
                body: None,
            });
        }
    }
}

const DONE_DEDUP_WINDOW_SECS: f64 = 3.0;

fn window_foreground(windows: &Query<&Window, With<bevy::window::PrimaryWindow>>) -> bool {
    windows
        .iter()
        .next()
        .map(|w| w.focused && w.visible)
        .unwrap_or(false)
}

fn agent_is_viewed(
    entity: Entity,
    foreground: bool,
    focused: &vmux_layout::stack::FocusedStack,
    child_of: &Query<&ChildOf>,
) -> bool {
    let stack = child_of.get(entity).ok().map(|c| c.parent());
    foreground && focused.stack.is_some() && focused.stack == stack
}

fn mark_agent_done(
    mut reader: MessageReader<vmux_core::notify::AgentAttention>,
    mut notify: MessageWriter<vmux_core::notify::OsNotify>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    focused: Res<vmux_layout::stack::FocusedStack>,
    child_of: Query<&ChildOf>,
    meta: Query<(
        &vmux_core::team::Profile,
        Option<&SessionId>,
        Option<&vmux_core::team::Agent>,
    )>,
    time: Res<Time>,
    mut last_notify: Local<std::collections::HashMap<Entity, f64>>,
    mut commands: Commands,
) {
    let foreground = window_foreground(&windows);
    for att in reader.read() {
        commands
            .entity(att.entity)
            .insert(vmux_core::notify::AgentDoneUnseen);
        if agent_is_viewed(att.entity, foreground, &focused, &child_of) {
            continue;
        }
        let now = time.elapsed_secs_f64();
        if last_notify
            .get(&att.entity)
            .is_some_and(|t| now - t < DONE_DEDUP_WINDOW_SECS)
        {
            continue;
        }
        last_notify.insert(att.entity, now);
        let (name, sid) = match meta.get(att.entity) {
            Ok((profile, session, agent)) => {
                let sid = session
                    .map(|s| s.0.clone())
                    .filter(|s| !s.is_empty())
                    .or_else(|| agent.map(|a| a.sid.clone()).filter(|s| !s.is_empty()))
                    .unwrap_or_default();
                (profile.name.clone(), sid)
            }
            Err(_) => ("Agent".to_string(), String::new()),
        };
        let short_sid: String = sid.chars().take(8).collect();
        let title = att
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{name} finished"));
        let body = att
            .body
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                if short_sid.is_empty() {
                    String::new()
                } else {
                    format!("session {short_sid}")
                }
            });
        notify.write(vmux_core::notify::OsNotify { title, body });
    }
}

fn clear_agent_done(
    done: Query<Entity, With<vmux_core::notify::AgentDoneUnseen>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    focused: Res<vmux_layout::stack::FocusedStack>,
    child_of: Query<&ChildOf>,
    mut prev_focused: Local<Option<Entity>>,
    mut commands: Commands,
) {
    let foreground = window_foreground(&windows);
    let current = if foreground { focused.stack } else { None };
    if current == *prev_focused {
        return;
    }
    *prev_focused = current;
    let Some(stack) = current else {
        return;
    };
    for entity in &done {
        if child_of.get(entity).ok().map(|c| c.parent()) == Some(stack) {
            commands
                .entity(entity)
                .remove::<vmux_core::notify::AgentDoneUnseen>();
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AgentBrowserResolve<'w, 's> {
    activate: MessageWriter<'w, vmux_layout::active_panes::ActivatePane>,
    // Matches any anchored content (CLI terminal or ACP chat webview) by its unique ProcessId.
    agent_terms: Query<
        'w,
        's,
        (
            Entity,
            &'static vmux_service::protocol::ProcessId,
            &'static ChildOf,
        ),
    >,
    kinds: Query<'w, 's, &'static AgentSession>,
    child_of: Query<'w, 's, &'static ChildOf>,
    pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    stack_q: Query<'w, 's, Entity, With<vmux_layout::stack::Stack>>,
    browser_stacks: Query<'w, 's, &'static ChildOf, With<vmux_layout::Browser>>,
}

impl AgentBrowserResolve<'_, '_> {
    /// The browser pane the agent opened beside itself: a sibling leaf pane
    /// (same parent split) that hosts a browser. Resolved from the layout tree,
    /// never from the user's `FocusedStack`.
    fn browser_pane_for(&self, agent_pane: Entity) -> Option<Entity> {
        use bevy::ecs::relationship::Relationship;
        let agent_parent = self.child_of.get(agent_pane).ok()?.get();
        for stack_co in self.browser_stacks.iter() {
            let pane = stack_co.get();
            if pane == agent_pane {
                continue;
            }
            if let Ok(parent_co) = self.child_of.get(pane)
                && parent_co.get() == agent_parent
                && self.pane_has_only_browser_stacks(pane)
            {
                return Some(pane);
            }
        }
        None
    }

    fn pane_has_only_browser_stacks(&self, pane: Entity) -> bool {
        self.pane_children
            .get(pane)
            .ok()
            .map(|children| {
                children
                    .iter()
                    .filter(|&child| self.stack_q.contains(child))
                    .all(|child| self.browser_stacks.contains(child))
            })
            .unwrap_or(false)
    }

    /// The agent's own pane (its stack's parent pane), from its anchor.
    fn agent_pane(&self, anchor: vmux_service::protocol::ProcessId) -> Option<Entity> {
        use bevy::ecs::relationship::Relationship;
        let (_, _, term_co) = self
            .agent_terms
            .iter()
            .find(|(_, pid, _)| **pid == anchor)?;
        self.child_of.get(term_co.get()).ok().map(|co| co.get())
    }

    /// The kind of the agent at `anchor` (Claude/Codex/Vibe), for its avatar badge.
    /// `None` for ACP sessions (no `AgentKind`).
    fn agent_kind(&self, anchor: vmux_service::protocol::ProcessId) -> Option<AgentKind> {
        let (entity, _, _) = self
            .agent_terms
            .iter()
            .find(|(_, pid, _)| **pid == anchor)?;
        self.kinds.get(entity).ok().map(|session| session.kind)
    }

    /// Resolve the agent's browser pane from its anchor, and record it as that
    /// agent's active pane (for its focus ring). Returns the pane entity, or
    /// `None` if the agent has no browser pane yet (caller keeps the default).
    fn claim_browser_pane(&mut self, anchor: vmux_service::protocol::ProcessId) -> Option<Entity> {
        let pane = self.browser_pane_for(self.agent_pane(anchor)?)?;
        let kind = self.agent_kind(anchor);
        self.activate
            .write(vmux_layout::active_panes::ActivatePane {
                profile: vmux_layout::active_panes::ProfileId::Agent(format!("{anchor:?}")),
                active: vmux_layout::active_panes::ActiveStack {
                    tab: None,
                    pane: Some(pane),
                    stack: None,
                    kind,
                },
            });
        Some(pane)
    }

    /// Returns the explicit pane if given, else the agent's resolved browser
    /// pane as a bare entity-bits string (the form `parse_pane_target` expects,
    /// matching explicit MCP targets after they reach this layer). Returns
    /// `None` if neither is available.
    fn resolve_pane(
        &mut self,
        pane: &Option<String>,
        anchor: &Option<vmux_service::protocol::ProcessId>,
    ) -> Option<String> {
        if pane.is_some() {
            return pane.clone();
        }
        let anchor = (*anchor)?;
        self.claim_browser_pane(anchor)
            .map(|p| p.to_bits().to_string())
    }

    /// Same as `resolve_pane` but reads the anchor from a command's origin, for
    /// agent browser commands (back/forward) that carry origin rather than a
    /// query anchor.
    fn command_pane(&mut self, pane: &Option<String>, origin: &CommandOrigin) -> Option<String> {
        let anchor = match origin {
            CommandOrigin::Agent { anchor, .. } => *anchor,
            _ => None,
        };
        self.resolve_pane(pane, &anchor)
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AgentFileResolve<'w, 's> {
    activate: MessageWriter<'w, vmux_layout::active_panes::ActivatePane>,
    open_beside: MessageWriter<'w, vmux_layout::OpenBesideRequest>,
    agent_terms: Query<
        'w,
        's,
        (
            Entity,
            &'static vmux_service::protocol::ProcessId,
            &'static ChildOf,
        ),
    >,
    kinds: Query<'w, 's, &'static AgentSession>,
    child_of: Query<'w, 's, &'static ChildOf>,
    file_pages: Query<'w, 's, (Entity, &'static ChildOf, &'static vmux_core::PageMetadata)>,
}

impl AgentFileResolve<'_, '_> {
    fn agent_pane(&self, anchor: vmux_service::protocol::ProcessId) -> Option<Entity> {
        use bevy::ecs::relationship::Relationship;
        let (_, _, term_co) = self
            .agent_terms
            .iter()
            .find(|(_, pid, _)| **pid == anchor)?;
        self.child_of.get(term_co.get()).ok().map(|co| co.get())
    }

    /// The kind of the agent at `anchor` (Claude/Codex/Vibe), for its avatar badge.
    /// `None` for ACP sessions (no `AgentKind`).
    fn agent_kind(&self, anchor: vmux_service::protocol::ProcessId) -> Option<AgentKind> {
        let (entity, _, _) = self
            .agent_terms
            .iter()
            .find(|(_, pid, _)| **pid == anchor)?;
        self.kinds.get(entity).ok().map(|session| session.kind)
    }

    /// The agent's existing `file://` follow-page (the page entity) and its leaf
    /// pane: a sibling pane (same parent split) hosting a file page. `None` if
    /// the agent has no file pane yet.
    fn file_page_for(&self, agent_pane: Entity) -> Option<(Entity, Entity)> {
        use bevy::ecs::relationship::Relationship;
        let agent_parent = self.child_of.get(agent_pane).ok()?.get();
        for (page, page_co, meta) in self.file_pages.iter() {
            if !meta.url.starts_with("file:") {
                continue;
            }
            let Ok(pane_co) = self.child_of.get(page_co.get()) else {
                continue;
            };
            let pane = pane_co.get();
            if pane == agent_pane {
                continue;
            }
            if let Ok(parent_co) = self.child_of.get(pane)
                && parent_co.get() == agent_parent
            {
                return Some((page, pane));
            }
        }
        None
    }

    /// The agent's follow-pane and every `file://` preview stack in it, with each
    /// stack's URL. Generalizes `file_page_for` (which returns only the first).
    /// `None` when the agent has no file follow-pane yet.
    #[allow(clippy::type_complexity)]
    fn file_stacks_for(
        &self,
        agent_pane: Entity,
    ) -> Option<(Entity, Vec<(Entity, Entity, String)>)> {
        use bevy::ecs::relationship::Relationship;
        let agent_parent = self.child_of.get(agent_pane).ok()?.get();
        let mut follow_pane = None;
        let mut stacks = Vec::new();
        for (page, page_co, meta) in self.file_pages.iter() {
            if !meta.url.starts_with("file:") {
                continue;
            }
            let stack = page_co.get();
            let Ok(pane_co) = self.child_of.get(stack) else {
                continue;
            };
            let pane = pane_co.get();
            if pane == agent_pane {
                continue;
            }
            if self.child_of.get(pane).ok().map(|c| c.get()) != Some(agent_parent) {
                continue;
            }
            follow_pane = Some(pane);
            stacks.push((stack, page, meta.url.clone()));
        }
        follow_pane.map(|p| (p, stacks))
    }
}

/// Build the `file://` URL for a touched file, encoding an optional goto/select
/// as a fragment the editor understands: `#L<line>` (scroll) or
/// `#L<line>:<col>-<end>` (scroll + highlight the match). `line` is 1-based;
/// `col`/`end_col` are 0-based.
fn file_touch_url(path: &str, line: Option<u32>, col: Option<u32>, end_col: Option<u32>) -> String {
    let mut url = url::Url::from_file_path(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{path}"));
    if let Some(l) = line {
        url.push_str(&format!("#L{l}"));
        if let (Some(c), Some(e)) = (col, end_col) {
            url.push_str(&format!(":{c}-{e}"));
        }
    }
    url
}

/// On an agent file read/edit, open the file in a `file://` pane beside that
/// agent and record it as the agent's active pane (its focus ring). The first
/// file spirals a new pane; later reads stack as new tabs on top of that pane
/// (placement `AddTab`), and re-reading the same file refocuses its tab. `focus`
/// only brings the new tab to the front of the file pane (it does not move the
/// human's focused pane), so it is set only once that pane already exists.
fn handle_agent_file_touch(
    mut reader: MessageReader<AgentCommandRequest>,
    mut resolve: AgentFileResolve,
    settings: Res<AppSettings>,
) {
    if !settings.agent.follow_files {
        for _ in reader.read() {}
        return;
    }
    for request in reader.read() {
        let ServiceAgentCommand::FileTouched {
            anchor,
            path,
            line,
            col,
            end_col,
            ..
        } = &request.command
        else {
            continue;
        };
        let Some(agent_pane) = resolve.agent_pane(*anchor) else {
            continue;
        };
        let existing = resolve.file_page_for(agent_pane);
        resolve.open_beside.write(vmux_layout::OpenBesideRequest {
            pane: agent_pane,
            direction: None,
            url: file_touch_url(path, *line, *col, *end_col),
            request_id: request.request_id.0,
            focus: requested_focus_for_origin(&request.origin, existing.is_some()),
        });
        if let Some((_, pane)) = existing {
            let kind = resolve.agent_kind(*anchor);
            resolve
                .activate
                .write(vmux_layout::active_panes::ActivatePane {
                    profile: vmux_layout::active_panes::ProfileId::Agent(format!("{anchor:?}")),
                    active: vmux_layout::active_panes::ActiveStack {
                        tab: None,
                        pane: Some(pane),
                        stack: None,
                        kind,
                    },
                });
        }
    }
}

/// Tidy one agent's `file://` follow-pane, anchored by its `ProcessId`: when the pane
/// holds more than `tidy_files_max` previews, keep changed files + the active one and
/// close the rest (silently if `tidy_files_auto`, else tag the pane for the confirm
/// dialog). Shared by the CLI (`AgentAttention`) and ACP (`AgentRunState` → `Idle`) triggers.
#[allow(clippy::too_many_arguments)]
fn tidy_follow_pane(
    anchor: vmux_service::protocol::ProcessId,
    settings: &AppSettings,
    resolve: &AgentFileResolve,
    last_activated: &Query<&vmux_core::LastActivatedAt>,
    pending: &Query<(), With<crate::tidy::PendingTidy>>,
    close: &mut MessageWriter<vmux_layout::CloseStackRequest>,
    commands: &mut Commands,
) {
    let Some(agent_pane) = resolve.agent_pane(anchor) else {
        return;
    };
    let Some((follow_pane, stacks)) = resolve.file_stacks_for(agent_pane) else {
        return;
    };
    if pending.get(follow_pane).is_ok() {
        return;
    }
    let mut repos: Vec<(std::path::PathBuf, std::collections::HashSet<String>)> = Vec::new();
    let rows: Vec<(Entity, i64, bool)> = stacks
        .iter()
        .map(|(stack, _page, url)| {
            let ts = last_activated.get(*stack).map(|t| t.0).unwrap_or(i64::MIN);
            let changed = crate::tidy::path_from_file_url(url)
                .map(|abs| crate::tidy::is_changed(&abs, &mut repos))
                .unwrap_or(false);
            (*stack, ts, changed)
        })
        .collect();
    let closable = crate::tidy::decide_closable(&rows, settings.agent.tidy_files_max);
    if closable.is_empty() {
        return;
    }
    if settings.agent.tidy_files_auto {
        for stack in closable {
            close.write(vmux_layout::CloseStackRequest { stack });
        }
        return;
    }
    // Show the in-UI banner on the follow-pane's active (kept) preview and remember the
    // closable set on the pane until the user answers (`on_tidy_action`).
    let count = closable.len() as u32;
    let active_page = stacks
        .iter()
        .max_by_key(|(stack, _, _)| last_activated.get(*stack).map(|t| t.0).unwrap_or(i64::MIN))
        .map(|(_, page, _)| *page);
    if let Some(page) = active_page {
        commands.trigger(bevy_cef::prelude::BinHostEmitEvent::from_rkyv(
            page,
            vmux_core::event::FILE_TIDY_PROMPT_EVENT,
            &vmux_core::event::FileTidyPromptEvent { count },
        ));
        commands
            .entity(follow_pane)
            .insert(crate::tidy::PendingTidy { closable });
    }
}

/// CLI agents: tidy on turn-end `AgentAttention` (the terminal bell), anchored by the
/// agent terminal's `ProcessId`.
fn tidy_on_agent_attention(
    mut reader: MessageReader<vmux_core::notify::AgentAttention>,
    settings: Res<AppSettings>,
    agents: Query<&vmux_service::protocol::ProcessId, With<vmux_core::team::Agent>>,
    resolve: AgentFileResolve,
    last_activated: Query<&vmux_core::LastActivatedAt>,
    pending: Query<(), With<crate::tidy::PendingTidy>>,
    mut close: MessageWriter<vmux_layout::CloseStackRequest>,
    mut commands: Commands,
) {
    if !settings.agent.tidy_files {
        for _ in reader.read() {}
        return;
    }
    for att in reader.read() {
        let Ok(pid) = agents.get(att.entity) else {
            continue;
        };
        tidy_follow_pane(
            *pid,
            &settings,
            &resolve,
            &last_activated,
            &pending,
            &mut close,
            &mut commands,
        );
    }
}

/// ACP agents have no terminal bell; their turn-end is `AgentRunState` → `Idle`. Tidy the
/// ACP follow-pane on that transition, anchored by the `AcpSession`.
fn tidy_acp_on_idle(
    settings: Res<AppSettings>,
    sessions: Query<
        (&crate::client::acp::AcpSession, &crate::AgentRunState),
        Changed<crate::AgentRunState>,
    >,
    resolve: AgentFileResolve,
    last_activated: Query<&vmux_core::LastActivatedAt>,
    pending: Query<(), With<crate::tidy::PendingTidy>>,
    mut close: MessageWriter<vmux_layout::CloseStackRequest>,
    mut commands: Commands,
) {
    if !settings.agent.tidy_files {
        return;
    }
    for (acp, state) in &sessions {
        if !matches!(state, crate::AgentRunState::Idle) {
            continue;
        }
        tidy_follow_pane(
            acp.anchor,
            &settings,
            &resolve,
            &last_activated,
            &pending,
            &mut close,
            &mut commands,
        );
    }
}

/// The user answered the follow-pane tidy banner (`FileTidyActionEvent` from the active
/// file page): close the remembered previews, and on "Always" persist
/// `agent.tidy_files_auto`; "Dismiss" just drops the pending set.
pub(crate) fn on_tidy_action(
    trigger: On<bevy_cef::prelude::BinReceive<vmux_core::event::FileTidyActionEvent>>,
    child_of: Query<&ChildOf>,
    pending: Query<&crate::tidy::PendingTidy>,
    mut settings: ResMut<AppSettings>,
    mut save: MessageWriter<vmux_setting::SettingsSaveRequest>,
    mut close: MessageWriter<vmux_layout::CloseStackRequest>,
    mut commands: Commands,
) {
    use bevy::ecs::relationship::Relationship;
    // The event comes from a file page webview: webview → stack → follow-pane (holds PendingTidy).
    let webview = trigger.event().webview;
    let Ok(stack) = child_of.get(webview).map(Relationship::get) else {
        return;
    };
    let Ok(pane) = child_of.get(stack).map(Relationship::get) else {
        return;
    };
    let Ok(pending_tidy) = pending.get(pane) else {
        return;
    };
    let closable = pending_tidy.closable.clone();
    commands.entity(pane).remove::<crate::tidy::PendingTidy>();
    match trigger.event().payload.choice {
        vmux_core::event::TidyChoice::Dismiss => {}
        vmux_core::event::TidyChoice::Always => {
            settings.agent.tidy_files_auto = true;
            save.write(vmux_setting::SettingsSaveRequest);
            for stack in closable {
                close.write(vmux_layout::CloseStackRequest { stack });
            }
        }
        vmux_core::event::TidyChoice::Tidy => {
            for stack in closable {
                close.write(vmux_layout::CloseStackRequest { stack });
            }
        }
    }
}

fn handle_agent_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    mut app_commands: MessageWriter<AppCommand>,
    mut browser_nav_writer: MessageWriter<vmux_layout::BrowserNavigateRequest>,
    mut browser_go_back_writer: MessageWriter<vmux_layout::BrowserGoBackRequest>,
    mut browser_go_forward_writer: MessageWriter<vmux_layout::BrowserGoForwardRequest>,
    mut stack_writers: (
        MessageWriter<vmux_layout::OpenInNewStackRequest>,
        MessageWriter<vmux_layout::ExtensionInstallRequest>,
    ),
    mut terminal_send_writer: MessageWriter<vmux_terminal::TerminalSendRequest>,
    mut run_shell_writer: MessageWriter<vmux_terminal::RunShellRequest>,
    mut terminal_stack_spawn_writer: MessageWriter<TerminalStackSpawnRequest>,
    mut process_stack_spawn_writer: MessageWriter<ProcessStackSpawnRequest>,
    focus: Res<FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    lookups: AgentLookups,
    mut sp: SettingsParams,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    mut writers: AgentSpaceWriters,
) {
    let active_space = lookups.active_space.as_deref();
    use vmux_service::protocol::{AgentCommandResult, ClientMessage};

    for request in reader.read() {
        let caller = match &request.origin {
            CommandOrigin::Agent {
                anchor: Some(pid), ..
            } => writers
                .agents
                .iter()
                .find(|(_, _, p)| p.is_some_and(|p| p == pid))
                .map(|(e, _, _)| e),
            CommandOrigin::Agent { sid: Some(sid), .. } if !sid.is_empty() => writers
                .agents
                .iter()
                .find(|(_, a, _)| &a.sid == sid)
                .map(|(e, _, _)| e),
            CommandOrigin::User => writers.user.single().ok(),
            _ => None,
        };
        let result = match &request.command {
            ServiceAgentCommand::FileTouched { .. } => AgentCommandResult::Ok,
            ServiceAgentCommand::AppCommand { id, args_json } => {
                let args: serde_json::Value = if args_json.is_empty() {
                    serde_json::json!({})
                } else {
                    serde_json::from_str(args_json).unwrap_or(serde_json::json!({}))
                };
                match AppCommand::from_mcp_call(id, args) {
                    Some(Ok(command)) => {
                        if origin_is_agent(&request.origin)
                            && !agent_may_dispatch_app_command(&command)
                        {
                            AgentCommandResult::Error(
                                "focus-changing app command is disabled for agents".to_string(),
                            )
                        } else {
                            if let Some(caller) = caller {
                                writers.issued.write(vmux_command::CommandIssued {
                                    caller,
                                    command: command.clone(),
                                });
                            }
                            app_commands.write(command);
                            AgentCommandResult::Ok
                        }
                    }
                    Some(Err(message)) => AgentCommandResult::Error(message),
                    None => match AppCommand::from_mcp_id(id) {
                        Some(command) => {
                            if origin_is_agent(&request.origin)
                                && !agent_may_dispatch_app_command(&command)
                            {
                                AgentCommandResult::Error(
                                    "focus-changing app command is disabled for agents".to_string(),
                                )
                            } else {
                                if let Some(caller) = caller {
                                    writers.issued.write(vmux_command::CommandIssued {
                                        caller,
                                        command: command.clone(),
                                    });
                                }
                                app_commands.write(command);
                                AgentCommandResult::Ok
                            }
                        }
                        None => AgentCommandResult::Error(format!("unknown app command: {id}")),
                    },
                }
            }
            ServiceAgentCommand::NewTerminalTab {
                cwd,
                command,
                args,
                env,
            } => match focus.pane.filter(|pane| panes.contains(*pane)) {
                None => AgentCommandResult::Error("no active pane".to_string()),
                Some(pane) => match valid_cwd(cwd) {
                    Err(message) => AgentCommandResult::Error(message),
                    Ok(cwd_opt) => {
                        let activate = !origin_is_agent(&request.origin);
                        let cwd_path = cwd_opt.unwrap_or_else(|| {
                            active_space
                                .as_ref()
                                .map(|s| {
                                    vmux_setting::resolve_startup_dir(&sp.settings, &s.record.id)
                                })
                                .unwrap_or_else(default_space_dir)
                        });
                        if command.trim().is_empty() {
                            terminal_stack_spawn_writer.write(TerminalStackSpawnRequest {
                                pane,
                                cwd: Some(cwd_path),
                                pending_input: None,
                                process_id: None,
                                activate,
                            });
                        } else {
                            process_stack_spawn_writer.write(ProcessStackSpawnRequest {
                                pane,
                                command: command.clone(),
                                args: args.clone(),
                                cwd: cwd_path,
                                env: env.clone(),
                                activate,
                            });
                        }
                        AgentCommandResult::Ok
                    }
                },
            },
            ServiceAgentCommand::RunShell { command, cwd, mode } => {
                let shell_mode = match mode {
                    AgentShellMode::Active => vmux_terminal::ShellMode::Active,
                    AgentShellMode::NewTab => vmux_terminal::ShellMode::NewTab,
                };
                run_shell_writer.write(vmux_terminal::RunShellRequest {
                    command: command.clone(),
                    cwd: cwd.clone(),
                    mode: shell_mode,
                });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::BrowserNavigate { url, pane } => {
                let mut pane = pane.clone();
                if pane.is_none()
                    && let CommandOrigin::Agent {
                        anchor: Some(anchor),
                        ..
                    } = &request.origin
                {
                    if let Some(browser_pane) = writers.browse.claim_browser_pane(*anchor) {
                        pane = Some(browser_pane.to_bits().to_string());
                    } else if let Some(agent_pane) = writers.browse.agent_pane(*anchor) {
                        writers.open_beside.write(vmux_layout::OpenBesideRequest {
                            pane: agent_pane,
                            direction: None,
                            url: url.clone(),
                            request_id: request.request_id.0,
                            focus: false,
                        });
                        continue;
                    } else {
                        if let Some(service) = service.as_ref() {
                            service.0.send(ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error(
                                    "browser_navigate: agent has no resolvable pane".to_string(),
                                ),
                            });
                        }
                        continue;
                    }
                }
                browser_nav_writer.write(vmux_layout::BrowserNavigateRequest {
                    url: url.clone(),
                    pane,
                    request_id: Some(request.request_id.0),
                });
                continue;
            }
            ServiceAgentCommand::BrowserInstallExtension { source } => {
                stack_writers.1.write(vmux_layout::ExtensionInstallRequest {
                    source: source.clone(),
                });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::TerminalSend { text, terminal } => {
                terminal_send_writer.write(vmux_terminal::TerminalSendRequest {
                    text: text.clone(),
                    terminal: terminal.clone(),
                });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::Notify { title, body } => match caller {
                Some(caller) => {
                    writers.attention.write(vmux_core::notify::AgentAttention {
                        entity: caller,
                        title: title.clone(),
                        body: body.clone(),
                    });
                    AgentCommandResult::Ok
                }
                None => AgentCommandResult::Error("notify: caller not found".to_string()),
            },
            ServiceAgentCommand::FocusPane { pane } => {
                if origin_is_agent(&request.origin) {
                    AgentCommandResult::Error("focus_pane is disabled for agents".to_string())
                } else {
                    writers
                        .focus_pane
                        .write(FocusPaneRequest { pane: pane.clone() });
                    AgentCommandResult::Ok
                }
            }
            ServiceAgentCommand::RenameProfile { name } => {
                writers
                    .rename_profile
                    .write(RenameProfileRequest { name: name.clone() });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::UpdateSettings { path, value_json } => {
                match serde_json::from_str::<serde_json::Value>(value_json) {
                    Ok(value) => {
                        match vmux_setting::apply_settings_update(sp.settings.as_mut(), path, value)
                        {
                            Ok(ron_bytes) => {
                                sp.writes
                                    .write(vmux_setting::SettingsWriteRequest { ron_bytes });
                                AgentCommandResult::Ok
                            }
                            Err(message) => AgentCommandResult::Error(message),
                        }
                    }
                    Err(e) => AgentCommandResult::Error(format!(
                        "update_settings: invalid JSON value: {e}"
                    )),
                }
            }
            ServiceAgentCommand::UpdateLayout { layout } => {
                let mut layout = layout.clone();
                if origin_is_agent(&request.origin) {
                    preserve_current_focus_in_layout_snapshot(&mut layout, &focus);
                }
                writers
                    .layout_apply
                    .write(vmux_layout::reconcile::LayoutApplyRequest {
                        request_id: request.request_id.0,
                        snapshot: layout,
                    });
                continue;
            }
            ServiceAgentCommand::BrowserGoBack { pane } => {
                let pane = writers.browse.command_pane(pane, &request.origin);
                browser_go_back_writer.write(vmux_layout::BrowserGoBackRequest { pane });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::BrowserGoForward { pane } => {
                let pane = writers.browse.command_pane(pane, &request.origin);
                browser_go_forward_writer.write(vmux_layout::BrowserGoForwardRequest { pane });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::BrowserHistorySearch { query, limit } => {
                bevy::log::info!("browser_history_search: query={:?} limit={}", query, limit);
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::OpenInNewStack { url } => {
                stack_writers
                    .0
                    .write(vmux_layout::OpenInNewStackRequest { url: url.clone() });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::SpaceCommand {
                command,
                space_id,
                name,
            } => {
                writers
                    .space_command
                    .write(vmux_space::SpaceCommandRequest {
                        command: command.clone(),
                        space_id: space_id.clone(),
                        name: name.clone(),
                    });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::OpenBeside { .. }
            | ServiceAgentCommand::Run { .. }
            | ServiceAgentCommand::CreateWorktree { .. } => {
                continue;
            }
        };
        if let Some(service) = service.as_ref() {
            service.0.send(ClientMessage::AgentCommandResponse {
                request_id: request.request_id,
                result,
            });
        }
    }
}

fn resolve_self_pane(
    anchor: ProcessId,
    agent_terms: &Query<(Entity, &ProcessId, &ChildOf)>,
    child_of_q: &Query<&ChildOf>,
) -> Option<(Entity, Entity)> {
    use bevy::ecs::relationship::Relationship;
    let (term, _, term_co) = agent_terms.iter().find(|(_, pid, _)| **pid == anchor)?;
    let stack = term_co.get();
    let pane = child_of_q.get(stack).ok()?.get();
    Some((term, pane))
}

/// The pane containing the terminal whose `ProcessId` is `pid` (its stack's
/// parent pane). Used to anchor a `run` next to an existing terminal page.
fn resolve_pane_for_pid(
    pid: ProcessId,
    term_pids: &Query<(Entity, &ProcessId), With<Terminal>>,
    child_of_q: &Query<&ChildOf>,
) -> Option<Entity> {
    use bevy::ecs::relationship::Relationship;
    let (term, _) = term_pids.iter().find(|(_, p)| **p == pid)?;
    let stack = child_of_q.get(term).ok()?.get();
    let pane = child_of_q.get(stack).ok()?.get();
    Some(pane)
}

fn tab_of_run_pane(
    pane: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<vmux_layout::tab::Tab>>,
) -> Option<Entity> {
    use bevy::ecs::relationship::Relationship;
    let mut cur = pane;
    for _ in 0..32 {
        if tab_q.contains(cur) {
            return Some(cur);
        }
        cur = child_of_q.get(cur).ok()?.get();
    }
    None
}

fn run_terminal_candidates(
    agent_pane: Entity,
    terminals: &Query<
        (Entity, &ProcessId),
        (
            With<Terminal>,
            Without<AgentSession>,
            Without<ProcessExited>,
        ),
    >,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<vmux_layout::tab::Tab>>,
    seq_q: &Query<&vmux_layout::pane::SpawnSeq>,
) -> Vec<RunTerminalCandidate> {
    use bevy::ecs::relationship::Relationship;
    let Some(agent_tab) = tab_of_run_pane(agent_pane, child_of_q, tab_q) else {
        return Vec::new();
    };
    terminals
        .iter()
        .filter_map(|(terminal, pid)| {
            let stack = child_of_q.get(terminal).ok()?.get();
            let pane = child_of_q.get(stack).ok()?.get();
            if pane == agent_pane {
                return None;
            }
            if tab_of_run_pane(pane, child_of_q, tab_q) != Some(agent_tab) {
                return None;
            }
            Some(RunTerminalCandidate {
                pid: *pid,
                stack,
                pane,
                pane_spawn_seq: seq_q.get(pane).map(|s| s.0).unwrap_or(0),
            })
        })
        .collect()
}

fn run_terminal_bucket_panes(
    agent_pane: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<vmux_layout::tab::Tab>>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_q: &Query<Entity, With<vmux_layout::stack::Stack>>,
    page_q: &Query<&PageMetadata, With<vmux_layout::stack::Stack>>,
    seq_q: &Query<&vmux_layout::pane::SpawnSeq>,
) -> Vec<RunTerminalBucketPaneCandidate> {
    let Some(agent_tab) = tab_of_run_pane(agent_pane, child_of_q, tab_q) else {
        return Vec::new();
    };
    leaf_panes
        .iter()
        .filter_map(|pane| {
            if pane == agent_pane {
                return None;
            }
            if tab_of_run_pane(pane, child_of_q, tab_q) != Some(agent_tab) {
                return None;
            }
            let children = pane_children.get(pane).ok()?;
            let mut has_stack = false;
            for stack in children.iter().filter(|&child| stack_q.contains(child)) {
                has_stack = true;
                let meta = page_q.get(stack).ok()?;
                if vmux_layout::placement::page_kind_for_url(&meta.url)
                    != vmux_layout::placement::PageKind::Terminal
                {
                    return None;
                }
            }
            has_stack.then(|| RunTerminalBucketPaneCandidate {
                pane,
                pane_spawn_seq: seq_q.get(pane).map(|s| s.0).unwrap_or(0),
            })
        })
        .collect()
}

fn newest_run_terminal_bucket_pane(
    agent_pane: Entity,
    candidates: &[RunTerminalBucketPaneCandidate],
) -> Option<Entity> {
    candidates
        .iter()
        .filter(|c| c.pane != agent_pane)
        .max_by_key(|c| c.pane_spawn_seq)
        .map(|c| c.pane)
}

fn is_run_terminal_bucket_pane(
    pane: Entity,
    candidates: &[RunTerminalBucketPaneCandidate],
) -> bool {
    candidates.iter().any(|c| c.pane == pane)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingRunTerminalSpawn {
    pid: ProcessId,
    request_index: usize,
}

fn append_pending_run_terminal_input(
    anchor: ProcessId,
    pending_spawns: &std::collections::HashMap<ProcessId, PendingRunTerminalSpawn>,
    terminal_spawns: &mut [TerminalStackSpawnRequest],
    data: &[u8],
) -> Option<ProcessId> {
    let pending = pending_spawns.get(&anchor)?;
    let request = terminal_spawns.get_mut(pending.request_index)?;
    match &mut request.pending_input {
        Some(input) => input.extend(data),
        None => request.pending_input = Some(data.to_vec()),
    }
    Some(pending.pid)
}

fn touch_reused_run_pane_spawn_seq(
    pane: Entity,
    commands: &mut Commands,
    spawn_counter: &mut vmux_layout::pane::SpawnCounter,
    seq_q: &Query<&vmux_layout::pane::SpawnSeq>,
) {
    let max_existing = seq_q.iter().map(|s| s.0).max().unwrap_or(0);
    if spawn_counter.0 <= max_existing {
        spawn_counter.0 = max_existing;
    }
    spawn_counter.0 += 1;
    commands
        .entity(pane)
        .insert(vmux_layout::pane::SpawnSeq(spawn_counter.0));
}

fn focus_reused_run_terminal(
    candidate: RunTerminalCandidate,
    commands: &mut Commands,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<vmux_layout::tab::Tab>>,
) {
    commands
        .entity(candidate.stack)
        .insert(LastActivatedAt::now());
    commands
        .entity(candidate.pane)
        .insert(LastActivatedAt::now());
    if let Some(tab) = tab_of_run_pane(candidate.pane, child_of_q, tab_q) {
        commands.entity(tab).insert(LastActivatedAt::now());
    }
}

/// Split `pane` and return the new leaf pane. Batches several splits of the same
/// pane in one tick (extend an existing split instead of re-splitting the leaf).
#[allow(clippy::too_many_arguments)]
fn split_pane_off(
    commands: &mut Commands,
    pane: Entity,
    direction: &vmux_service::protocol::AgentPaneDirection,
    focus: bool,
    pane_children: &Query<&Children, With<Pane>>,
    tab_filter: &Query<Entity, With<vmux_layout::stack::Stack>>,
    split_dir_q: &Query<&PaneSplit>,
    split_this_batch: &mut std::collections::HashSet<Entity>,
) -> Entity {
    let existing_tabs: Vec<Entity> = pane_children
        .get(pane)
        .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
        .unwrap_or_default();
    let split_dir = vmux_layout::pane::direction_to_split(&to_pane_direction(direction));
    let already_split = !split_this_batch.insert(pane) || split_dir_q.contains(pane);
    vmux_layout::pane::split_or_extend(
        commands,
        pane,
        split_dir,
        &existing_tabs,
        focus,
        already_split,
    )
}

fn to_pane_direction(
    d: &vmux_service::protocol::AgentPaneDirection,
) -> vmux_command::open::PaneDirection {
    use vmux_command::open::PaneDirection;
    use vmux_service::protocol::AgentPaneDirection as D;
    match d {
        D::Top => PaneDirection::Top,
        D::Right => PaneDirection::Right,
        D::Bottom => PaneDirection::Bottom,
        D::Left => PaneDirection::Left,
    }
}

pub(crate) fn agent_terminal_shell(settings: &AppSettings) -> String {
    settings
        .terminal
        .as_ref()
        .map(|t| t.resolve_theme(&t.default_theme).shell)
        .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string()))
}

/// Wrap a `run` command so the shell emits an invisible OSC completion escape
/// carrying the exit code once the command finishes (success OR failure).
/// `token` is a unique per-run id; the escape is
/// `ESC ] <VMUX_RUN_OSC> ; <token> ; <exit_code> BEL` (see
/// [`vmux_service::run_marker`]). Because it is an OSC sequence the terminal
/// parser consumes it — it never renders as text, unlike the old
/// `__VMUX_DONE_…__` printf markers.
///
/// The command is prefixed with [`pager_env_prefix`] so an interactive command that would
/// normally open a pager (e.g. `git log` → `less`) prints straight to the terminal instead of
/// blocking the marker forever.
///
/// posix/fish chain with `;` (which continues after a non-zero command). nushell
/// aborts the rest of a `;` line when an external command fails, so it needs a
/// `try`/`catch` wrapper to always emit the escape and recover the exit code
/// from the caught error.
fn command_with_marker(shell: &str, command: &str, token: &str) -> String {
    let base = std::path::Path::new(shell)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(shell);
    let pager = pager_env_prefix(base);
    let osc = vmux_service::run_marker::VMUX_RUN_OSC;
    match base {
        "nu" | "nushell" => format!(
            "{pager}try {{ {command}; print -rn $\"\\u{{1b}}]{osc};{token};($env.LAST_EXIT_CODE)\\u{{7}}\" }} catch {{ |e| print -rn $\"\\u{{1b}}]{osc};{token};($e.exit_code? | default 1)\\u{{7}}\" }}"
        ),
        "fish" => format!(
            "{pager}{command}; set __vmux_status $status; printf '\\033]{osc};{token};%s\\007' $__vmux_status"
        ),
        _ => format!(
            "{pager}{command}; __vmux_status=\"$?\"; printf '\\033]{osc};{token};%s\\007' \"$__vmux_status\""
        ),
    }
}

/// Shell-specific prelude that neutralizes pagers for a `run`, so an interactive command can't
/// stall the completion marker waiting on `less` (`git log`, `man`, `git diff`, …). Set as
/// session-exported env so follow-up runs in the same shell stay covered.
fn pager_env_prefix(base: &str) -> &'static str {
    match base {
        "nu" | "nushell" => "$env.GIT_PAGER = \"cat\"; $env.PAGER = \"cat\"; $env.LESS = \"FRX\"; ",
        "fish" => "set -gx GIT_PAGER cat; set -gx PAGER cat; set -gx LESS FRX; ",
        _ => "export GIT_PAGER=cat PAGER=cat LESS=FRX; ",
    }
}

fn run_command_line(command: &str, token: Option<&str>, settings: &AppSettings) -> String {
    match token {
        Some(token) => command_with_marker(&agent_terminal_shell(settings), command, token),
        None => command.to_string(),
    }
}

fn run_terminal_cwd(agent_launch_cwd: Option<&str>, active_space: Option<&ActiveSpace>) -> PathBuf {
    if let Some(Ok(Some(path))) = agent_launch_cwd.map(valid_cwd) {
        return path;
    }
    active_space
        .map(|s| space_dir(&s.record.id))
        .unwrap_or_else(default_space_dir)
}

#[allow(clippy::too_many_arguments)]
fn handle_agent_self_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    agent_terms: Query<(Entity, &ProcessId, &ChildOf)>,
    term_pids: Query<(Entity, &ProcessId), With<Terminal>>,
    run_terms: Query<
        (Entity, &ProcessId),
        (
            With<Terminal>,
            Without<AgentSession>,
            Without<ProcessExited>,
        ),
    >,
    launch_q: Query<&TerminalLaunch>,
    ctx: vmux_layout::pane::PlacementCtx,
    mut open_beside_writer: MessageWriter<vmux_layout::OpenBesideRequest>,
    mut terminal_stack_spawn_writer: MessageWriter<TerminalStackSpawnRequest>,
    mut commands: Commands,
    service: Option<Res<ServiceClient>>,
    active_space: Option<Res<ActiveSpace>>,
    settings: Res<AppSettings>,
    mut regions: ResMut<AgentTerminalRegions>,
    mut spawn_counter: ResMut<vmux_layout::pane::SpawnCounter>,
    mut tabs: Query<&mut vmux_layout::tab::Tab>,
    tab_worktrees: Query<&vmux_layout::tab::TabWorktree>,
) {
    use vmux_service::protocol::{AgentCommandResult, ClientMessage};
    let Some(service) = service else {
        for _ in reader.read() {}
        return;
    };
    // Anchors split during this batch. Several `run`s dispatched in one tick all
    // resolve to the same agent pane; the first splits it, the rest must extend
    // that split rather than re-split the leaf (which would orphan empty panes).
    let mut split_this_batch: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    let mut worktree_created_this_batch: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    let mut terminal_spawns: Vec<TerminalStackSpawnRequest> = Vec::new();
    let mut pending_run_spawns: std::collections::HashMap<ProcessId, PendingRunTerminalSpawn> =
        std::collections::HashMap::new();
    for request in reader.read() {
        let result = match &request.command {
            ServiceAgentCommand::OpenBeside {
                anchor,
                direction,
                url,
                focus,
            } => match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("self process not found".to_string()),
                Some((_, pane)) => {
                    let focus = requested_focus_for_origin(&request.origin, *focus);
                    open_beside_writer.write(vmux_layout::OpenBesideRequest {
                        pane,
                        direction: direction.as_ref().map(to_pane_direction),
                        url: url.clone(),
                        request_id: request.request_id.0,
                        focus,
                    });
                    AgentCommandResult::Ok
                }
            },
            ServiceAgentCommand::Run {
                anchor,
                command,
                direction,
                focus,
                beside,
                mode,
                terminal,
                done_marker,
            } => {
                let focus = requested_focus_for_origin(&request.origin, *focus);
                let mut data =
                    run_command_line(command, done_marker.as_deref(), &settings).into_bytes();
                data.push(b'\r');
                match terminal {
                    Some(pid) => {
                        service.0.send(ClientMessage::ProcessInput {
                            process_id: *pid,
                            data,
                        });
                        AgentCommandResult::Text(pid.to_string())
                    }
                    None => 'spawn: {
                        let Some((agent_term, self_pane)) =
                            resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q)
                        else {
                            break 'spawn AgentCommandResult::Error(
                                "self process not found".to_string(),
                            );
                        };
                        let candidates = run_terminal_candidates(
                            self_pane,
                            &run_terms,
                            &ctx.child_of_q,
                            &ctx.tab_q,
                            &ctx.seq_q,
                        );
                        let terminal_bucket_panes = run_terminal_bucket_panes(
                            self_pane,
                            &ctx.child_of_q,
                            &ctx.tab_q,
                            &ctx.leaf_panes,
                            &ctx.pane_children,
                            &ctx.tab_filter,
                            &ctx.page_q,
                            &ctx.seq_q,
                        );
                        if beside.is_none()
                            && *mode == vmux_service::protocol::PlacementMode::Auto
                            && let Some(pid) = append_pending_run_terminal_input(
                                *anchor,
                                &pending_run_spawns,
                                &mut terminal_spawns,
                                &data,
                            )
                        {
                            break 'spawn AgentCommandResult::Text(pid.to_string());
                        }
                        if beside.is_none()
                            && *mode == vmux_service::protocol::PlacementMode::Auto
                            && let Some(candidate) = choose_reusable_run_terminal(
                                *anchor,
                                self_pane,
                                &regions,
                                &candidates,
                            )
                        {
                            regions.run_terminals.insert(*anchor, candidate.pid);
                            regions.run_panes.insert(*anchor, candidate.pane);
                            touch_reused_run_pane_spawn_seq(
                                candidate.pane,
                                &mut commands,
                                &mut spawn_counter,
                                &ctx.seq_q,
                            );
                            if focus {
                                focus_reused_run_terminal(
                                    candidate,
                                    &mut commands,
                                    &ctx.child_of_q,
                                    &ctx.tab_q,
                                );
                            }
                            service.0.send(ClientMessage::ProcessInput {
                                process_id: candidate.pid,
                                data,
                            });
                            break 'spawn AgentCommandResult::Text(candidate.pid.to_string());
                        }
                        // Resolve an explicit `beside` anchor up front (errors if stale).
                        let beside_pane = match beside {
                            Some(pid) => {
                                match resolve_pane_for_pid(*pid, &term_pids, &ctx.child_of_q) {
                                    Some(pane) => Some(pane),
                                    None => {
                                        break 'spawn AgentCommandResult::Error(format!(
                                            "run.beside page not found: {pid}"
                                        ));
                                    }
                                }
                            }
                            None => None,
                        };
                        use vmux_service::protocol::PlacementMode;
                        let target_pane = match (beside_pane, *mode) {
                            (anchor_pane, PlacementMode::Split) => {
                                let bucket_pane = if anchor_pane.is_none() {
                                    choose_run_terminal_bucket_pane(
                                        *anchor,
                                        self_pane,
                                        &regions,
                                        &candidates,
                                    )
                                    .filter(|pane| {
                                        is_run_terminal_bucket_pane(*pane, &terminal_bucket_panes)
                                    })
                                    .or_else(|| {
                                        newest_run_terminal_bucket_pane(
                                            self_pane,
                                            &terminal_bucket_panes,
                                        )
                                    })
                                } else {
                                    None
                                };
                                if let Some(pane) = bucket_pane {
                                    pane
                                } else {
                                    let anchor_pane = anchor_pane.unwrap_or_else(|| {
                                        vmux_layout::pane::resolve_split_anchor_pane(
                                            self_pane, &ctx,
                                        )
                                    });
                                    split_pane_off(
                                        &mut commands,
                                        anchor_pane,
                                        direction,
                                        focus,
                                        &ctx.pane_children,
                                        &ctx.tab_filter,
                                        &ctx.split_dir_q,
                                        &mut split_this_batch,
                                    )
                                }
                            }
                            (Some(pane), _) => pane,
                            (None, _) => vmux_layout::pane::resolve_spiral_pane(
                                &mut commands,
                                self_pane,
                                TERMINAL_PAGE_URL,
                                focus,
                                &mut split_this_batch,
                                &ctx,
                            ),
                        };
                        touch_reused_run_pane_spawn_seq(
                            target_pane,
                            &mut commands,
                            &mut spawn_counter,
                            &ctx.seq_q,
                        );
                        let new_pid = ProcessId::new();
                        let agent_cwd = launch_q.get(agent_term).ok().map(|l| l.cwd.clone());
                        let cwd = run_terminal_cwd(agent_cwd.as_deref(), active_space.as_deref());
                        let request_index = terminal_spawns.len();
                        terminal_spawns.push(TerminalStackSpawnRequest {
                            pane: target_pane,
                            cwd: Some(cwd),
                            pending_input: Some(data),
                            process_id: Some(new_pid),
                            activate: focus,
                        });
                        regions.run_panes.insert(*anchor, target_pane);
                        if beside.is_none() && *mode != vmux_service::protocol::PlacementMode::Split
                        {
                            regions.run_terminals.insert(*anchor, new_pid);
                            pending_run_spawns.insert(
                                *anchor,
                                PendingRunTerminalSpawn {
                                    pid: new_pid,
                                    request_index,
                                },
                            );
                        }
                        AgentCommandResult::Text(new_pid.to_string())
                    }
                }
            }
            ServiceAgentCommand::CreateWorktree { anchor } => {
                match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                    None => AgentCommandResult::Error("agent pane not found".to_string()),
                    Some((_, pane)) => {
                        let mut cur = pane;
                        let tab_e = loop {
                            if tabs.get(cur).is_ok() {
                                break Some(cur);
                            }
                            match ctx.child_of_q.get(cur) {
                                Ok(co) => cur = co.parent(),
                                Err(_) => break None,
                            }
                        };
                        match tab_e {
                            None => AgentCommandResult::Error("no tab for agent".to_string()),
                            Some(tab_e)
                                if tab_worktrees.get(tab_e).is_ok()
                                    || worktree_created_this_batch.contains(&tab_e) =>
                            {
                                let dir = tabs
                                    .get(tab_e)
                                    .ok()
                                    .and_then(|t| t.startup_dir.clone())
                                    .unwrap_or_default();
                                AgentCommandResult::Text(dir)
                            }
                            Some(tab_e) => {
                                let tab_dir =
                                    tabs.get(tab_e).ok().and_then(|t| t.startup_dir.clone());
                                let name =
                                    tabs.get(tab_e).map(|t| t.name.clone()).unwrap_or_default();
                                let space_id = active_space
                                    .as_deref()
                                    .map(|s| s.record.id.clone())
                                    .unwrap_or_default();
                                let base_dir = vmux_setting::resolve_startup_dir_for_tab(
                                    &settings,
                                    &space_id,
                                    tab_dir.as_deref(),
                                );
                                match vmux_layout::worktree::create_worktree_blocking(
                                    &base_dir, &name,
                                ) {
                                    Ok(info) => {
                                        let path = info.path.to_string_lossy().into_owned();
                                        if let Ok(mut t) = tabs.get_mut(tab_e) {
                                            t.startup_dir = Some(path.clone());
                                        }
                                        commands.entity(tab_e).insert(
                                            vmux_layout::tab::TabWorktree {
                                                repo_root: info
                                                    .repo_root
                                                    .to_string_lossy()
                                                    .into_owned(),
                                                branch: info.branch.clone(),
                                                base_ref: info.base_ref.clone(),
                                            },
                                        );
                                        worktree_created_this_batch.insert(tab_e);
                                        AgentCommandResult::Text(path)
                                    }
                                    Err(e) => AgentCommandResult::Error(e),
                                }
                            }
                        }
                    }
                }
            }
            _ => continue,
        };
        service.0.send(ClientMessage::AgentCommandResponse {
            request_id: request.request_id,
            result,
        });
    }
    for spawn in terminal_spawns {
        terminal_stack_spawn_writer.write(spawn);
    }
}

fn respond_process_stack_spawn(
    mut reader: MessageReader<ProcessStackSpawnRequest>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for request in reader.read() {
        let stack_ts = if request.activate {
            LastActivatedAt::now()
        } else {
            LastActivatedAt(0)
        };
        let stack = commands
            .spawn((
                vmux_layout::stack::stack_bundle(),
                stack_ts,
                ChildOf(request.pane),
            ))
            .id();
        let title = std::path::Path::new(&request.command)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&request.command)
            .to_string();
        commands.entity(stack).insert(PageMetadata {
            url: TERMINAL_PAGE_URL.to_string(),
            title,
            bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
            ..default()
        });
        let launch = vmux_terminal::launch::TerminalLaunch {
            command: request.command.clone(),
            args: request.args.clone(),
            cwd: request.cwd.to_string_lossy().to_string(),
            env: request.env.clone(),
            kind: vmux_terminal::launch::TerminalKind::Plain,
        };
        let term = commands
            .spawn((
                new_terminal_bundle_with_cwd(
                    &mut meshes,
                    &mut webview_mt,
                    &settings,
                    Some(&request.cwd),
                ),
                ChildOf(stack),
            ))
            .id();
        commands.entity(term).insert((launch, CefKeyboardTarget));
    }
}

#[allow(clippy::type_complexity)]
pub fn detect_agent_session_process_exit(
    mut commands: Commands,
    mut writer: MessageWriter<AgentSessionExited>,
    mut q: Query<
        (Entity, Option<&vmux_terminal::pid::Pid>, &mut PageMetadata),
        (With<AgentSession>, With<ProcessExited>),
    >,
    child_of: Query<&ChildOf>,
) {
    use bevy::ecs::relationship::Relationship;
    for (entity, pid, mut meta) in &mut q {
        commands
            .entity(entity)
            .remove::<AgentSession>()
            .remove::<SessionId>()
            .remove::<PendingAgentSession>()
            .remove::<vmux_core::team::Agent>()
            .remove::<vmux_core::team::Profile>();
        // A vibe agent terminal that exits should close its pane entirely, not
        // linger as a shell/terminal. The terminal is a child of a stack, which
        // is a child of a pane; mark that pane for a no-dialog force close. If
        // the pane can't be resolved, fall back to converting to a terminal.
        let pane = child_of
            .get(entity)
            .ok()
            .map(Relationship::get)
            .and_then(|stack| child_of.get(stack).ok())
            .map(Relationship::get);
        match pane {
            Some(pane) => {
                commands.entity(pane).insert(ForcePaneClose);
            }
            None => {
                let next = match pid {
                    Some(vmux_terminal::pid::Pid(p)) => {
                        format!("{}{p}", vmux_terminal::event::TERMINAL_PAGE_URL)
                    }
                    None => vmux_terminal::event::TERMINAL_PAGE_URL.to_string(),
                };
                if meta.url != next {
                    meta.url = next;
                }
            }
        }
        writer.write(AgentSessionExited { entity });
    }
}

pub(crate) fn forward_history_open_intent(
    mut intents: MessageReader<vmux_history::query::HistoryOpenIntent>,
    mut requests: MessageWriter<AgentCommandRequest>,
) {
    for intent in intents.read() {
        let command = if intent.in_new_stack {
            ServiceAgentCommand::OpenInNewStack {
                url: intent.url.clone(),
            }
        } else {
            ServiceAgentCommand::BrowserNavigate {
                url: intent.url.clone(),
                pane: None,
            }
        };
        requests.write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::User,
            command,
        });
    }
}

fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    settings: Res<AppSettings>,
    spaces: Query<
        (
            &vmux_layout::space::SpaceId,
            &Name,
            Has<vmux_core::Active>,
            Option<&vmux_core::Order>,
        ),
        With<vmux_layout::space::Space>,
    >,
    mut layout_snapshot_writer: MessageWriter<vmux_layout::reconcile::LayoutSnapshotRequest>,
    mut screenshot_writer: MessageWriter<ScreenshotRequest>,
    mut browser_snapshot_writer: MessageWriter<BrowserSnapshotRequest>,
    mut browser_scroll_writer: MessageWriter<BrowserScrollRequest>,
    mut record_start_writer: MessageWriter<RecordStartRequest>,
    mut record_stop_writer: MessageWriter<RecordStopRequest>,
    mut browse: AgentBrowserResolve,
) {
    let Some(service) = service else { return };

    for request in reader.read() {
        match request.query {
            AgentQuery::ReadLayout { anchor } => {
                layout_snapshot_writer.write(vmux_layout::reconcile::LayoutSnapshotRequest {
                    request_id: request.request_id.0,
                    anchor,
                });
            }
            AgentQuery::GetSettings => {
                let result =
                    AgentQueryResult::Settings(vmux_setting::serialize_settings_to_json(&settings));
                service.0.send(ClientMessage::AgentQueryResponse {
                    request_id: request.request_id,
                    result,
                });
            }
            AgentQuery::ListSpaces => {
                let mut rows: Vec<(u32, serde_json::Value)> = spaces
                    .iter()
                    .map(|(id, name, is_active, order)| {
                        (
                            order.map(|o| o.0).unwrap_or(u32::MAX),
                            serde_json::json!({
                                "id": id.0,
                                "name": name.to_string(),
                                "profile": vmux_space::model::bootstrap_profile_name(),
                                "is_active": is_active,
                            }),
                        )
                    })
                    .collect();
                rows.sort_by_key(|(order, _)| *order);
                let rows: Vec<serde_json::Value> = rows.into_iter().map(|(_, row)| row).collect();
                let json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string());
                service.0.send(ClientMessage::AgentQueryResponse {
                    request_id: request.request_id,
                    result: AgentQueryResult::Spaces(json),
                });
            }
            AgentQuery::Screenshot { ref pane } => {
                screenshot_writer.write(ScreenshotRequest {
                    request_id: request.request_id.0,
                    pane: pane.clone(),
                });
            }
            AgentQuery::BrowserSnapshot {
                ref pane,
                ref anchor,
            } => {
                browser_snapshot_writer.write(BrowserSnapshotRequest {
                    request_id: request.request_id.0,
                    pane: browse.resolve_pane(pane, anchor),
                });
            }
            AgentQuery::BrowserScroll {
                ref pane,
                ref to,
                delta,
                ref anchor,
            } => {
                browser_scroll_writer.write(BrowserScrollRequest {
                    request_id: request.request_id.0,
                    pane: browse.resolve_pane(pane, anchor),
                    to: to.clone(),
                    delta,
                });
            }
            AgentQuery::RecordStart {
                gif,
                max_secs,
                ref pane,
            } => {
                record_start_writer.write(RecordStartRequest {
                    request_id: request.request_id.0,
                    gif,
                    max_secs,
                    pane: pane.clone(),
                });
            }
            AgentQuery::RecordStop { ref dir, ref name } => {
                record_stop_writer.write(RecordStopRequest {
                    request_id: request.request_id.0,
                    dir: dir.clone(),
                    name: name.clone(),
                });
            }
            // ReadTerminal/ReadTerminalFull/CommandExit/RunCompletion are
            // answered by the service directly; they never reach the GUI.
            AgentQuery::ReadTerminal { .. }
            | AgentQuery::ReadTerminalFull { .. }
            | AgentQuery::CommandExit { .. }
            | AgentQuery::RunCompletion { .. } => {}
        }
    }
}

fn forward_layout_apply_responses(
    mut reader: MessageReader<vmux_layout::reconcile::LayoutApplyResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        let result = match response.result.clone() {
            Ok(snapshot) => AgentCommandResult::Layout(snapshot),
            Err(message) => AgentCommandResult::Error(message),
        };
        service.0.send(ClientMessage::AgentCommandResponse {
            request_id: AgentRequestId(response.request_id),
            result,
        });
    }
}

fn forward_layout_snapshot_responses(
    mut reader: MessageReader<vmux_layout::reconcile::LayoutSnapshotResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: AgentQueryResult::Layout(response.snapshot.clone()),
        });
    }
}

fn screenshot_response_to_query_result(
    result: &Result<ScreenshotImage, String>,
) -> AgentQueryResult {
    match result {
        Ok(img) => AgentQueryResult::Image {
            path: img.path.clone(),
            png: img.png.clone(),
            width: img.width,
            height: img.height,
        },
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}

fn forward_screenshot_responses(
    mut reader: MessageReader<ScreenshotResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: screenshot_response_to_query_result(&response.result),
        });
    }
}

fn forward_snapshot_responses(
    mut reader: MessageReader<BrowserSnapshotResponse>,
    service: Option<Res<ServiceClient>>,
    mut nav_awaiting: ResMut<NavAwaitingSnapshot>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        if nav_awaiting.0.remove(&response.request_id) {
            let result = match &response.result {
                Ok(json) => AgentCommandResult::Text(json.clone()),
                Err(message) => AgentCommandResult::Error(message.clone()),
            };
            service.0.send(ClientMessage::AgentCommandResponse {
                request_id: AgentRequestId(response.request_id),
                result,
            });
        } else {
            service.0.send(ClientMessage::AgentQueryResponse {
                request_id: AgentRequestId(response.request_id),
                result: snapshot_response_to_query_result(&response.result),
            });
        }
    }
}

fn record_start_response_to_query_result(result: &Result<u32, String>) -> AgentQueryResult {
    match result {
        Ok(max_secs) => AgentQueryResult::Text(format!("recording started, max {max_secs}s")),
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}

fn forward_record_start_responses(
    mut reader: MessageReader<RecordStartResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: record_start_response_to_query_result(&response.result),
        });
    }
}

fn record_stop_response_to_query_result(
    result: &Result<RecordingInfo, String>,
) -> AgentQueryResult {
    match result {
        Ok(info) => AgentQueryResult::Recording {
            mp4_path: info.mp4_path.clone(),
            gif_path: info.gif_path.clone(),
            duration_ms: info.duration_ms,
            bytes: info.bytes,
            auto_stopped: info.auto_stopped,
        },
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}

fn forward_record_stop_responses(
    mut reader: MessageReader<RecordStopResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: record_stop_response_to_query_result(&response.result),
        });
    }
}

fn handle_agent_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    agents: Query<&vmux_core::agent::AgentSession>,
    acp_sessions: Query<&crate::client::acp::AcpSession>,
    child_of_q: Query<&ChildOf>,
    agent_to_entity: Option<Res<AgentSessionToEntity>>,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
    mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: Res<AppSettings>,
    active_space: Option<Res<ActiveSpace>>,
    tabs: Query<&vmux_layout::tab::Tab>,
    catalog: Option<Res<crate::client::acp::AcpCatalog>>,
) {
    for (entity, task) in &tasks {
        if !task.url.starts_with("vmux://agent/") {
            continue;
        }
        let tab_dir = vmux_layout::tab::ancestor_tab_startup_dir(task.stack, &child_of_q, &tabs);
        let default_cwd = active_space
            .as_deref()
            .map(|s| {
                vmux_setting::resolve_startup_dir_for_tab(
                    &settings,
                    &s.record.id,
                    tab_dir.as_deref(),
                )
            })
            .unwrap_or_else(default_space_dir);
        match handle_agent_page_open_task(
            task,
            &children_q,
            &agents,
            &acp_sessions,
            &child_of_q,
            agent_to_entity.as_deref(),
            idx.as_deref(),
            &kind_q,
            &mut spawn_agent,
            &mut commands,
            &mut meshes,
            &mut webview_mt,
            &default_cwd,
            &settings.agent.acp,
            catalog.as_deref(),
        ) {
            Ok(()) => {
                commands.entity(entity).insert(PageOpenHandled);
            }
            Err(message) => {
                commands.entity(entity).insert(PageOpenError { message });
            }
        }
    }
}

fn handle_agent_page_open_task(
    task: &PageOpenTask,
    children_q: &Query<&Children>,
    agents: &Query<&vmux_core::agent::AgentSession>,
    acp_sessions: &Query<&crate::client::acp::AcpSession>,
    child_of_q: &Query<&ChildOf>,
    agent_to_entity: Option<&AgentSessionToEntity>,
    idx: Option<&crate::client::page::strategy_index::PageStrategyIndex>,
    kind_q: &Query<&crate::client::page::strategy_components::StrategyKind>,
    spawn_agent: &mut MessageWriter<SpawnAgentInStackRequest>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    default_cwd: &std::path::Path,
    acp_configs: &[vmux_setting::AcpAgentConfig],
    catalog: Option<&crate::client::acp::AcpCatalog>,
) -> Result<(), String> {
    if let Some(kind) = AgentKind::all()
        .into_iter()
        .find(|k| task.url == k.setup_url())
    {
        attach_cli_setup_to_stack(kind, task.stack, children_q, commands, meshes, webview_mt);
        return Ok(());
    }
    match crate::AgentUrl::parse(&task.url) {
        Some(crate::AgentUrl::Page {
            provider,
            model,
            sid,
        }) => {
            clear_stack_children(task.stack, children_q, commands);
            let idx = idx.ok_or_else(|| "page strategy index not registered".to_string())?;
            attach_page_agent_to_stack(
                task.stack, &provider, &model, &sid, commands, meshes, webview_mt, idx, kind_q,
            )
            .ok_or_else(|| format!("no Page agent strategy registered for {provider}/{model}"))?;
            Ok(())
        }
        Some(crate::AgentUrl::PageDefault) => {
            let provider = crate::providers::resolve_default_app_provider().ok_or_else(|| {
                "no default Page agent provider available (set MISTRAL_API_KEY, ANTHROPIC_API_KEY, or OPENAI_API_KEY)"
                    .to_string()
            })?;
            let idx = idx.ok_or_else(|| "page strategy index not registered".to_string())?;
            let sid = uuid::Uuid::new_v4().to_string();
            clear_stack_children(task.stack, children_q, commands);
            attach_page_agent_to_stack(
                task.stack,
                provider.provider,
                provider.default_model,
                &sid,
                commands,
                meshes,
                webview_mt,
                idx,
                kind_q,
            )
            .ok_or_else(|| {
                format!(
                    "no Page agent strategy registered for {}/{}",
                    provider.provider, provider.default_model
                )
            })?;
            Ok(())
        }
        Some(crate::AgentUrl::Cli { kind, sid }) => {
            if sid == crate::url::CLI_FRESH_SID {
                if !stack_has_agent_of_kind(task.stack, kind, children_q, agents) {
                    spawn_agent.write(SpawnAgentInStackRequest {
                        kind,
                        cwd: default_cwd.to_path_buf(),
                        session_id: None,
                        stack: task.stack,
                        initial_prompt: None,
                    });
                }
                return Ok(());
            }
            if let Some(map) = agent_to_entity
                && let Some(&entity) = map.0.get(&(kind, sid.clone()))
            {
                vmux_terminal::pid::focus_pane_entity(entity, commands, child_of_q);
                return Ok(());
            }
            spawn_agent.write(SpawnAgentInStackRequest {
                kind,
                cwd: default_cwd.to_path_buf(),
                session_id: Some(sid),
                stack: task.stack,
                initial_prompt: None,
            });
            Ok(())
        }
        Some(crate::AgentUrl::Acp { id, sid }) => {
            // ACP agents own the canonical single-segment names (claude/codex/…) plus the
            // two-segment `<id>/<acp-session-id>` session form.
            let Some(cfg) = acp_configs.iter().find(|c| c.id == id) else {
                // Not an ACP agent. A bare `vmux://agent/<kind>` for a built-in CLI kind falls
                // back to a fresh CLI session (CLI's own url is `<kind>/cli`); this keeps the
                // legacy bare-url entry point (and the missing-binary setup flow) working.
                if sid.is_none()
                    && let Some(kind) = AgentKind::from_url_segment(&id)
                {
                    if !stack_has_agent_of_kind(task.stack, kind, children_q, agents) {
                        spawn_agent.write(SpawnAgentInStackRequest {
                            kind,
                            cwd: default_cwd.to_path_buf(),
                            session_id: None,
                            stack: task.stack,
                            initial_prompt: None,
                        });
                    }
                    return Ok(());
                }
                return Err(format!("no ACP agent configured for '{id}'"));
            };
            // Already attached to this agent on this stack? A repeat open (or the post-spawn url
            // redirect) is a no-op instead of re-spawning the session.
            if acp_sessions
                .get(task.stack)
                .is_ok_and(|session| session.agent_id == cfg.id)
            {
                return Ok(());
            }
            clear_stack_children(task.stack, children_q, commands);
            // `sid` (when present) is the agent-assigned ACP session id from a restored url — pass
            // it as the resume target. Fresh opens mint a routing sid and load nothing.
            let routing_sid = uuid::Uuid::new_v4().to_string();
            let icon = acp_icon_for_id(catalog, &cfg.id);
            attach_acp_agent_to_stack(
                task.stack,
                &cfg.id,
                &cfg.name,
                &routing_sid,
                default_cwd,
                icon.as_deref(),
                sid.as_deref(),
                commands,
                meshes,
                webview_mt,
            );
            Ok(())
        }
        None => Err(format!("malformed agent URL '{}'", task.url)),
    }
}

fn stack_has_agent_of_kind(
    stack: Entity,
    kind: AgentKind,
    children_q: &Query<&Children>,
    agents: &Query<&vmux_core::agent::AgentSession>,
) -> bool {
    children_q
        .get(stack)
        .map(|children| {
            children
                .iter()
                .any(|child| agents.get(child).is_ok_and(|session| session.kind == kind))
        })
        .unwrap_or(false)
}

pub(crate) fn clear_stack_children(
    stack: Entity,
    children_q: &Query<&Children>,
    commands: &mut Commands,
) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

fn attach_agent_spawn_error_to_stack(
    stack: Entity,
    kind: AgentKind,
    message: &str,
    children_q: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    clear_stack_children(stack, children_q, commands);
    let title = "Agent failed to start";
    let url = format!("vmux://error/agent/{}/", kind.as_url_segment());
    let message = html_escape(message);
    let html = format!(
        "<!doctype html><html><head><meta charset='utf-8'><title>{title}</title><style>html,body{{height:100%;margin:0;background:#101114;color:#e8e8ea;font-family:-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif}}main{{height:100%;display:flex;align-items:center;justify-content:center;padding:40px;box-sizing:border-box}}section{{max-width:640px}}h1{{font-size:28px;line-height:1.15;margin:0 0 12px;font-weight:650}}p{{font-size:14px;line-height:1.55;margin:0;color:#a9abb2}}code{{display:block;margin-top:18px;padding:12px;border-radius:6px;background:#1a1c22;color:#d7d8dd;white-space:pre-wrap;word-break:break-word}}</style></head><body><main><section><h1>{title}</h1><p>{}</p><code>{}</code></section></main></body></html>",
        kind.display_name(),
        message
    );
    let data_url = data_url_for_html(&html);
    commands.entity(stack).insert(PageMetadata {
        url,
        title: title.to_string(),
        bg_color: Some("#101114".to_string()),
        ..default()
    });
    let browser = commands
        .spawn((
            vmux_layout::Browser::new_with_title(meshes, webview_mt, &data_url, title),
            ChildOf(stack),
        ))
        .id();
    commands.entity(browser).insert(CefKeyboardTarget);
}

fn attach_cli_setup_to_stack(
    kind: AgentKind,
    stack: Entity,
    children_q: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    clear_stack_children(stack, children_q, commands);
    commands
        .entity(stack)
        .remove::<crate::vibe::setup::AgentSetupNavigated>();
    let title = format!("Set up {} CLI", kind.display_name());
    let url = kind.setup_url();
    commands.entity(stack).insert(PageMetadata {
        url: url.clone(),
        title: title.clone(),
        bg_color: Some("#101114".to_string()),
        ..default()
    });
    let browser = commands
        .spawn((
            vmux_layout::Browser::new_with_title(meshes, webview_mt, &url, &title),
            ChildOf(stack),
        ))
        .id();
    commands.entity(browser).insert(CefKeyboardTarget);
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn data_url_for_html(html: &str) -> String {
    let mut encoded = String::with_capacity(html.len() * 3);
    for byte in html.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    format!("data:text/html;charset=utf-8,{encoded}")
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_spawn_agent_requests(
    mut reader: MessageReader<SpawnAgentInStackRequest>,
    settings: Res<AppSettings>,
    strategies: Option<Res<AgentStrategies>>,
    exec_override: Option<Res<AgentExecutableOverride>>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for req in reader.read() {
        let Some(strategies) = strategies.as_deref() else {
            let message = "agent strategies not registered; cannot spawn agent";
            bevy::log::warn!("{message}");
            attach_agent_spawn_error_to_stack(
                req.stack,
                req.kind,
                message,
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            continue;
        };
        let Some(exe_path) = resolve_agent_executable(req.kind, exec_override.as_deref()) else {
            attach_cli_setup_to_stack(
                req.kind,
                req.stack,
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            continue;
        };
        let process_id = ProcessId::new();
        match crate::build_agent_launch(
            req.kind,
            &req.cwd,
            req.session_id.as_deref(),
            strategies,
            &exe_path,
            process_id,
        ) {
            Ok(launch) => {
                clear_stack_children(req.stack, &children_q, &mut commands);
                let terminal = commands
                    .spawn((
                        new_terminal_bundle_with_cwd(
                            &mut meshes,
                            &mut webview_mt,
                            &settings,
                            Some(&req.cwd),
                        ),
                        ChildOf(req.stack),
                    ))
                    .id();
                commands.entity(terminal).insert(CefKeyboardTarget).insert((
                    launch,
                    AgentSession { kind: req.kind },
                    process_id,
                    vmux_core::team::Profile::agent(req.kind),
                    vmux_core::team::Agent {
                        sid: req.session_id.clone().unwrap_or_default(),
                        kind: Some(req.kind),
                    },
                ));
                if let Some(id) = req.session_id.clone() {
                    commands.entity(terminal).insert(SessionId(id));
                } else {
                    commands.entity(terminal).insert(PendingAgentSession {
                        kind: req.kind,
                        spawn_time: std::time::SystemTime::now(),
                        cwd: req.cwd.clone(),
                    });
                }
                if let Some(prompt) = req.initial_prompt.clone().filter(|p| !p.trim().is_empty()) {
                    commands
                        .entity(terminal)
                        .insert(vmux_terminal::BufferedAgentPrompt {
                            text: prompt,
                            submit: true,
                        });
                }
            }
            Err(e) => {
                bevy::log::warn!("agent spawn ({:?}) failed: {e}", req.kind);
                attach_agent_spawn_error_to_stack(
                    req.stack,
                    req.kind,
                    &e,
                    &children_q,
                    &mut commands,
                    &mut meshes,
                    &mut webview_mt,
                );
            }
        }
    }
}

fn respond_page_agent_attach(
    mut reader: MessageReader<PageAgentAttachRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
) {
    for req in reader.read() {
        let Some(idx) = idx.as_deref() else {
            bevy::log::warn!("page strategy index not registered; skipping page attach");
            continue;
        };
        let _ = attach_page_agent_to_stack(
            req.stack,
            &req.provider,
            &req.model,
            &req.sid,
            &mut commands,
            &mut meshes,
            &mut webview_mt,
            idx,
            &kind_q,
        );
    }
}

fn respond_page_agent_spawn_stack(
    mut reader: MessageReader<PageAgentSpawnStackRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
) {
    for req in reader.read() {
        let Some(idx) = idx.as_deref() else {
            bevy::log::warn!("page strategy index not registered; skipping page spawn");
            continue;
        };
        let stack = commands
            .spawn((
                vmux_layout::stack::stack_bundle(),
                LastActivatedAt::now(),
                ChildOf(req.pane),
            ))
            .id();
        let _ = attach_page_agent_to_stack(
            stack,
            &req.provider,
            &req.model,
            &req.sid,
            &mut commands,
            &mut meshes,
            &mut webview_mt,
            idx,
            &kind_q,
        );
    }
}

fn respond_page_agent_spawn_default(
    mut reader: MessageReader<PageAgentSpawnDefaultRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
) {
    for req in reader.read() {
        let Some(idx) = idx.as_deref() else {
            bevy::log::warn!("page strategy index not registered; skipping default page spawn");
            continue;
        };
        let Some(p) = crate::providers::resolve_default_app_provider() else {
            bevy::log::warn!(
                "no default Page agent provider available (set MISTRAL_API_KEY, ANTHROPIC_API_KEY, or OPENAI_API_KEY)"
            );
            continue;
        };
        let sid = uuid::Uuid::new_v4().to_string();
        let stack = commands
            .spawn((
                vmux_layout::stack::stack_bundle(),
                LastActivatedAt::now(),
                ChildOf(req.pane),
            ))
            .id();
        if attach_page_agent_to_stack(
            stack,
            p.provider,
            p.default_model,
            &sid,
            &mut commands,
            &mut meshes,
            &mut webview_mt,
            idx,
            &kind_q,
        )
        .is_none()
        {
            bevy::log::warn!(
                "page agent stack spawn failed: no strategy registered for {}/{}",
                p.provider,
                p.default_model
            );
        }
    }
}

fn respond_page_agent_attach_default(
    mut reader: MessageReader<PageAgentAttachDefaultRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
) {
    for req in reader.read() {
        let Some(idx) = idx.as_deref() else {
            bevy::log::warn!("page strategy index not registered; skipping default page attach");
            continue;
        };
        let Some(p) = crate::providers::resolve_default_app_provider() else {
            bevy::log::warn!(
                "no default Page agent provider available (set MISTRAL_API_KEY, ANTHROPIC_API_KEY, or OPENAI_API_KEY)"
            );
            continue;
        };
        let sid = uuid::Uuid::new_v4().to_string();
        if attach_page_agent_to_stack(
            req.stack,
            p.provider,
            p.default_model,
            &sid,
            &mut commands,
            &mut meshes,
            &mut webview_mt,
            idx,
            &kind_q,
        )
        .is_none()
        {
            bevy::log::warn!(
                "attach_page_agent_to_stack returned None: no strategy registered for {}/{}",
                p.provider,
                p.default_model
            );
        }
    }
}

fn rebuilt_args_env_for_restart(
    launch: &TerminalLaunch,
    strategy: &dyn crate::client::cli::strategy::CliAgentStrategy,
    session_id: Option<&str>,
    new_id: ProcessId,
) -> (Vec<String>, Vec<(String, String)>) {
    let Ok(mcp_cfg) = crate::mcp::resolve(std::path::Path::new(&launch.cwd), new_id) else {
        return (launch.args.clone(), launch.env.clone());
    };
    let args = strategy.build_args(&mcp_cfg, session_id);
    let fresh = strategy.build_env(&mcp_cfg);
    let fresh_keys: std::collections::HashSet<String> =
        fresh.iter().map(|(k, _)| k.clone()).collect();
    let mut env: Vec<(String, String)> = launch
        .env
        .iter()
        .filter(|(k, _)| !fresh_keys.contains(k))
        .cloned()
        .collect();
    env.extend(fresh);
    (args, env)
}

fn handle_restart_agent_pty(
    mut reader: MessageReader<RestartAgentPty>,
    mut q: Query<(
        &mut ProcessId,
        Option<&mut TerminalLaunch>,
        &AgentSession,
        Option<&SessionId>,
        Option<&TerminalGridSize>,
    )>,
    service: Option<Res<ServiceClient>>,
    strategies: Option<Res<AgentStrategies>>,
    mut commands: Commands,
) {
    let Some(service) = service else {
        for _ in reader.read() {}
        return;
    };
    for msg in reader.read() {
        let Ok((mut pid, mut launch, session, session_id, grid)) = q.get_mut(msg.entity) else {
            continue;
        };
        service
            .0
            .send(ClientMessage::KillProcess { process_id: *pid });
        let new_id = ProcessId::new();

        let (command, args, cwd, env) = match launch.as_deref() {
            Some(l) => {
                let (rebuilt_args, rebuilt_env) =
                    match strategies.as_deref().and_then(|s| s.get_cli(session.kind)) {
                        Some(strategy) => rebuilt_args_env_for_restart(
                            l,
                            strategy,
                            session_id.map(|s| s.0.as_str()),
                            new_id,
                        ),
                        None => (l.args.clone(), l.env.clone()),
                    };
                (l.command.clone(), rebuilt_args, l.cwd.clone(), rebuilt_env)
            }
            None => (String::new(), vec![], String::new(), Vec::new()),
        };

        let (cols, rows) = grid.map(|g| (g.cols, g.rows)).unwrap_or((80, 24));
        service.0.send(ClientMessage::CreateProcess {
            process_id: new_id,
            command: command.clone(),
            args: args.clone(),
            cwd: cwd.clone(),
            env: env.clone(),
            cols,
            rows,
        });

        *pid = new_id;
        commands.entity(msg.entity).insert(AwaitingProcessCreated);
        if let Some(l) = launch.as_mut() {
            l.args = args;
            l.env = env;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_attach_gives_profile_agent_and_icon() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();
        let stack = app.world_mut().spawn_empty().id();

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut mt: ResMut<Assets<WebviewExtendStandardMaterial>>| {
                    attach_acp_agent_to_stack(
                        stack,
                        "mistral-vibe",
                        "Mistral Vibe",
                        "sid-1",
                        std::path::Path::new("/tmp"),
                        Some("https://cdn.example/vibe.svg"),
                        None,
                        &mut commands,
                        &mut meshes,
                        &mut mt,
                    );
                },
            )
            .unwrap();

        let world = app.world();
        let profile = world
            .get::<vmux_core::team::Profile>(stack)
            .expect("profile");
        assert_eq!(profile.name, "Mistral Vibe");
        let agent = world.get::<vmux_core::team::Agent>(stack).expect("agent");
        assert_eq!(agent.sid, "sid-1");
        assert_eq!(agent.kind, None);
        let meta = world.get::<PageMetadata>(stack).expect("meta");
        assert_eq!(meta.icon.favicon_url(), "https://cdn.example/vibe.svg");
    }

    #[test]
    fn acp_icon_for_id_reads_catalog() {
        use crate::acp_registry::{Distribution, RegistryAgent};
        let catalog = crate::client::acp::AcpCatalog {
            agents: vec![RegistryAgent {
                id: "mistral-vibe".to_string(),
                name: "Mistral Vibe".to_string(),
                version: None,
                description: None,
                icon: Some("https://cdn.example/vibe.svg".to_string()),
                repository: None,
                distribution: Distribution::default(),
            }],
        };
        assert_eq!(
            acp_icon_for_id(Some(&catalog), "mistral-vibe").as_deref(),
            Some("https://cdn.example/vibe.svg")
        );
        assert_eq!(acp_icon_for_id(Some(&catalog), "absent"), None);
        assert_eq!(acp_icon_for_id(None, "mistral-vibe"), None);
    }
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{BrowserSettings, ShortcutSettings};
    use vmux_terminal::Terminal;

    #[test]
    fn file_touch_url_builds_goto_fragment() {
        assert_eq!(
            file_touch_url("/a/b.rs", None, None, None),
            "file:///a/b.rs"
        );
        assert_eq!(
            file_touch_url("/a/b.rs", Some(10), None, None),
            "file:///a/b.rs#L10"
        );
        assert_eq!(
            file_touch_url("/a/b.rs", Some(10), Some(5), Some(12)),
            "file:///a/b.rs#L10:5-12"
        );
    }

    fn bell_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_core::notify::BellReceived>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_systems(Update, agent_bell_to_attention);
        app
    }

    fn spawn_agent_with_pid(app: &mut App, pid: vmux_service::protocol::ProcessId) -> Entity {
        app.world_mut()
            .spawn((
                vmux_core::team::Agent {
                    sid: "s".to_string(),
                    kind: Some(vmux_core::agent::AgentKind::Claude),
                },
                pid,
            ))
            .id()
    }

    fn attentions(app: &App) -> Vec<Entity> {
        let messages = app
            .world()
            .resource::<bevy::ecs::message::Messages<vmux_core::notify::AgentAttention>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|a| a.entity).collect()
    }

    #[test]
    fn bell_resolves_to_agent_attention() {
        use vmux_service::protocol::ProcessId;
        let mut app = bell_test_app();
        let pid = ProcessId::new();
        let agent = spawn_agent_with_pid(&mut app, pid);
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::BellReceived>>()
            .write(vmux_core::notify::BellReceived { process_id: pid });
        app.update();
        assert_eq!(attentions(&app), vec![agent]);
    }

    #[test]
    fn bell_unknown_process_id_emits_nothing() {
        use vmux_service::protocol::ProcessId;
        let mut app = bell_test_app();
        let _agent = spawn_agent_with_pid(&mut app, ProcessId::new());
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::BellReceived>>()
            .write(vmux_core::notify::BellReceived {
                process_id: ProcessId::new(),
            });
        app.update();
        assert!(attentions(&app).is_empty());
    }

    fn done_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_message::<vmux_core::notify::OsNotify>()
            .init_resource::<vmux_layout::stack::FocusedStack>()
            .add_systems(Update, (mark_agent_done, clear_agent_done));
        app
    }

    fn spawn_agent_in_stack(app: &mut App) -> (Entity, Entity) {
        let stack = app.world_mut().spawn_empty().id();
        let agent = app
            .world_mut()
            .spawn((
                vmux_core::team::Profile::agent(vmux_core::agent::AgentKind::Claude),
                ChildOf(stack),
            ))
            .id();
        (agent, stack)
    }

    fn set_window(app: &mut App, focused: bool) {
        app.world_mut().spawn((
            Window {
                focused,
                visible: true,
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));
    }

    fn os_notify_count(app: &App) -> usize {
        let messages = app
            .world()
            .resource::<bevy::ecs::message::Messages<vmux_core::notify::OsNotify>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).count()
    }

    fn send_attention(app: &mut App, entity: Entity) {
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::AgentAttention>>()
            .write(vmux_core::notify::AgentAttention {
                entity,
                title: None,
                body: None,
            });
    }

    #[test]
    fn done_notifies_and_marks_when_backgrounded() {
        let mut app = done_test_app();
        let (agent, _stack) = spawn_agent_in_stack(&mut app);
        set_window(&mut app, false);
        send_attention(&mut app, agent);
        app.update();
        assert!(
            app.world()
                .get::<vmux_core::notify::AgentDoneUnseen>(agent)
                .is_some()
        );
        assert_eq!(os_notify_count(&app), 1);
    }

    #[test]
    fn no_banner_when_foreground_but_dot_still_set() {
        let mut app = done_test_app();
        let (agent, stack) = spawn_agent_in_stack(&mut app);
        set_window(&mut app, true);
        app.world_mut()
            .resource_mut::<vmux_layout::stack::FocusedStack>()
            .stack = Some(stack);
        app.update();
        send_attention(&mut app, agent);
        app.update();
        assert!(
            app.world()
                .get::<vmux_core::notify::AgentDoneUnseen>(agent)
                .is_some(),
            "dot shows even when foreground"
        );
        assert_eq!(os_notify_count(&app), 0, "no banner when foreground");
    }

    #[test]
    fn clear_removes_marker_on_focus_transition() {
        let mut app = done_test_app();
        let (agent, stack) = spawn_agent_in_stack(&mut app);
        set_window(&mut app, true);
        app.world_mut()
            .entity_mut(agent)
            .insert(vmux_core::notify::AgentDoneUnseen);
        app.update();
        assert!(
            app.world()
                .get::<vmux_core::notify::AgentDoneUnseen>(agent)
                .is_some()
        );
        app.world_mut()
            .resource_mut::<vmux_layout::stack::FocusedStack>()
            .stack = Some(stack);
        app.update();
        assert!(
            app.world()
                .get::<vmux_core::notify::AgentDoneUnseen>(agent)
                .is_none()
        );
    }

    #[test]
    fn screenshot_response_maps_ok_and_err() {
        let ok = screenshot_response_to_query_result(&Ok(ScreenshotImage {
            path: "/tmp/a.png".into(),
            png: vec![9, 8, 7],
            width: 10,
            height: 20,
        }));
        assert!(matches!(
            ok,
            AgentQueryResult::Image { path, png, width, height }
                if path == "/tmp/a.png" && png == vec![9, 8, 7] && width == 10 && height == 20
        ));

        let err = screenshot_response_to_query_result(&Err("nope".to_string()));
        assert!(matches!(err, AgentQueryResult::Error(m) if m == "nope"));
    }

    #[test]
    fn record_start_response_maps_ok_and_err() {
        let ok = record_start_response_to_query_result(&Ok(120));
        assert!(matches!(ok, AgentQueryResult::Text(t) if t.contains("120")));
        let err = record_start_response_to_query_result(&Err("nope".to_string()));
        assert!(matches!(err, AgentQueryResult::Error(m) if m == "nope"));
    }

    #[test]
    fn record_stop_response_maps_ok_and_err() {
        let ok = record_stop_response_to_query_result(&Ok(RecordingInfo {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: None,
            duration_ms: 1000,
            bytes: 42,
            auto_stopped: false,
        }));
        assert!(
            matches!(ok, AgentQueryResult::Recording { mp4_path, .. } if mp4_path == "/tmp/x.mp4")
        );
        let err = record_stop_response_to_query_result(&Err("boom".to_string()));
        assert!(matches!(err, AgentQueryResult::Error(m) if m == "boom"));
    }

    pub(super) fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 0.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
    }

    #[test]
    fn blank_cwd_is_accepted() {
        assert_eq!(valid_cwd("").unwrap(), None);
    }

    #[test]
    fn restart_rebuilds_args_with_new_anchor() {
        let temp = std::env::temp_dir().join(format!("vmux-restart-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(temp.join("Cargo.toml"), b"[workspace]\n").unwrap();
        let launch = TerminalLaunch {
            command: "/usr/local/bin/claude".into(),
            args: vec!["--mcp-config".into(), "OLD".into()],
            cwd: temp.to_string_lossy().to_string(),
            env: vec![],
            kind: vmux_core::terminal::TerminalKind::Claude,
        };
        let new_id = ProcessId::new();
        let (args, _env) = rebuilt_args_env_for_restart(
            &launch,
            &crate::client::cli::claude::ClaudeStrategy,
            None,
            new_id,
        );
        let _ = std::fs::remove_dir_all(&temp);
        let joined = args.join(" ");
        assert!(joined.contains("--anchor"), "args carry --anchor: {joined}");
        assert!(joined.contains(&new_id.to_string()), "anchor is the new id");
        assert!(!joined.contains("OLD"), "old args replaced");
    }

    #[test]
    fn deep_link_focuses_existing_claude_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<crate::session::AgentSessionToEntity>()
            .add_systems(Update, crate::session::track_session_id_inserts);

        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Claude,
                },
                SessionId("dl-1".into()),
            ))
            .id();

        app.update();

        let map = app
            .world()
            .resource::<crate::session::AgentSessionToEntity>();
        assert_eq!(
            map.0.get(&(AgentKind::Claude, "dl-1".into())),
            Some(&entity)
        );
    }

    #[test]
    fn agent_plugin_registers_all_three_provider_entries() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
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
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
        let strategies = app.world().resource::<AgentStrategies>();
        assert!(strategies.get_cli(AgentKind::Vibe).is_some());
        assert!(strategies.get_cli(AgentKind::Claude).is_some());
        assert!(strategies.get_cli(AgentKind::Codex).is_some());
    }

    #[test]
    fn update_settings_via_apply_mutates_resource_and_returns_ron() {
        let mut settings = test_settings();
        let ron_bytes = vmux_setting::apply_settings_update(
            &mut settings,
            "browser.startup_url",
            serde_json::json!("https://example.com/custom"),
        )
        .expect("apply ok");
        assert_eq!(settings.browser.startup_url, "https://example.com/custom");
        // sparse RON includes only sections that differ from the embedded
        // defaults; this override differs, so it appears.
        assert!(ron_bytes.contains("https://example.com/custom"));
    }

    #[test]
    fn terminal_send_writes_raw_text_to_active_terminal() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin))
            .add_message::<vmux_layout::BrowserNavigateRequest>()
            .add_message::<vmux_layout::BrowserGoBackRequest>()
            .add_message::<vmux_layout::BrowserGoForwardRequest>()
            .add_message::<vmux_layout::OpenInNewStackRequest>()
            .add_message::<vmux_layout::ExtensionInstallRequest>()
            .add_message::<vmux_layout::OpenBesideRequest>()
            .add_message::<vmux_layout::reconcile::LayoutApplyRequest>()
            .add_message::<vmux_layout::reconcile::LayoutApplyResponse>()
            .add_message::<vmux_layout::reconcile::LayoutSnapshotRequest>()
            .add_message::<vmux_layout::reconcile::LayoutSnapshotResponse>()
            .add_message::<vmux_terminal::TerminalSendRequest>()
            .add_message::<vmux_terminal::RunShellRequest>()
            .add_message::<vmux_setting::SettingsWriteRequest>()
            .add_message::<vmux_space::SpaceCommandRequest>()
            .add_message::<vmux_history::query::HistoryOpenIntent>()
            .add_systems(Update, vmux_terminal::handle_terminal_send_requests)
            .insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .insert(ChildOf(pane))
            .id();
        let terminal = app
            .world_mut()
            .spawn((Terminal, ProcessId::new()))
            .insert(ChildOf(stack))
            .id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::User,
                command: ServiceAgentCommand::TerminalSend {
                    text: "ls".to_string(),
                    terminal: None,
                },
            });

        app.update();
        app.update();

        let pending = app
            .world()
            .get::<vmux_terminal::PendingTerminalInput>(terminal)
            .expect("PendingTerminalInput inserted");
        assert_eq!(pending.data, b"ls".to_vec());
    }

    #[test]
    fn missing_vibe_cli_shows_setup_page_at_vibe_url() {
        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(VibeStrategy));
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(strategies)
            .insert_resource(AgentExecutableOverride(std::collections::HashMap::from([
                (AgentKind::Vibe, false),
            ])))
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(
                Update,
                (handle_agent_page_open, handle_spawn_agent_requests).chain(),
            );

        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        let child = app.world_mut().spawn(ChildOf(stack)).id();
        app.world_mut().spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://agent/vibe/".to_string(),
            request_id: None,
        });

        app.update();
        app.update();

        assert!(app.world().get_entity(child).is_err());
        let stack_meta = app.world().get::<PageMetadata>(stack).unwrap();
        assert_eq!(stack_meta.url, "vmux://agent/vibe/setup");
        assert_eq!(stack_meta.title, "Set up Vibe CLI");
        let mut browsers = app
            .world_mut()
            .query_filtered::<(&PageMetadata, &ChildOf), With<vmux_layout::Browser>>();
        let metas: Vec<PageMetadata> = browsers
            .iter(app.world())
            .filter(|(_, child_of)| child_of.parent() == stack)
            .map(|(meta, _)| meta.clone())
            .collect();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].title, "Set up Vibe CLI");
        assert_eq!(metas[0].url, "vmux://agent/vibe/setup");
    }

    #[test]
    fn missing_claude_or_codex_cli_shows_setup_page() {
        for (kind, segment) in [(AgentKind::Claude, "claude"), (AgentKind::Codex, "codex")] {
            // Isolate the legacy CLI path: ACP now shadows claude/codex single-segment URLs.
            let mut settings = test_settings();
            settings.agent.acp.clear();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<SpawnAgentInStackRequest>()
                .insert_resource(AgentStrategies::default())
                .insert_resource(AgentExecutableOverride(std::collections::HashMap::from([
                    (kind, false),
                ])))
                .insert_resource(settings)
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>()
                .add_systems(
                    Update,
                    (handle_agent_page_open, handle_spawn_agent_requests).chain(),
                );

            let stack = app
                .world_mut()
                .spawn(vmux_layout::stack::stack_bundle())
                .id();
            app.world_mut().spawn(PageOpenTask {
                id: vmux_core::PageOpenId::new(),
                stack,
                url: format!("vmux://agent/{segment}/"),
                request_id: None,
            });

            app.update();
            app.update();

            let stack_meta = app.world().get::<PageMetadata>(stack).unwrap();
            assert_eq!(stack_meta.url, format!("vmux://agent/{segment}/setup"));
            assert_eq!(
                stack_meta.title,
                format!("Set up {} CLI", kind.display_name())
            );
        }
    }

    #[test]
    fn explicit_setup_url_attaches_setup_page() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(AgentStrategies::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_agent_page_open);

        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://agent/codex/setup".to_string(),
            request_id: None,
        });

        app.update();
        app.update();

        let stack_meta = app.world().get::<PageMetadata>(stack).unwrap();
        assert_eq!(stack_meta.url, "vmux://agent/codex/setup");
        assert_eq!(stack_meta.title, "Set up Codex CLI");
    }

    #[test]
    fn fresh_claude_page_uses_space_startup_dir() {
        let dir = std::env::temp_dir().join(format!("vmux-startup-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut settings = test_settings();
        // Isolate the legacy CLI path: ACP now shadows the `claude` single-segment URL.
        settings.agent.acp.clear();
        settings.spaces.insert(
            "space-1".into(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(dir.to_string_lossy().into()),
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(settings)
            .insert_resource(vmux_space::spaces::ActiveSpace {
                record: vmux_space::model::SpaceRecord {
                    id: "space-1".into(),
                    name: "Space 1".into(),
                    profile: "Personal".into(),
                },
            })
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_agent_page_open);

        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://agent/claude/".to_string(),
            request_id: None,
        });

        app.update();

        let spawns: Vec<SpawnAgentInStackRequest> = app
            .world_mut()
            .resource_mut::<Messages<SpawnAgentInStackRequest>>()
            .drain()
            .collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(spawns.len(), 1, "one agent spawn emitted");
        assert_eq!(spawns[0].kind, AgentKind::Claude);
        assert_eq!(
            spawns[0].cwd, dir,
            "claude page cwd resolves to space startup_dir"
        );
    }

    #[test]
    fn fresh_claude_page_prefers_ancestor_tab_startup_dir() {
        let space_dir = std::env::temp_dir().join(format!("vmux-space-dir-{}", std::process::id()));
        let tab_dir = std::env::temp_dir().join(format!("vmux-tab-dir-{}", std::process::id()));
        std::fs::create_dir_all(&space_dir).unwrap();
        std::fs::create_dir_all(&tab_dir).unwrap();

        let mut settings = test_settings();
        settings.agent.acp.clear();
        settings.spaces.insert(
            "space-1".into(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(space_dir.to_string_lossy().into()),
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(settings)
            .insert_resource(vmux_space::spaces::ActiveSpace {
                record: vmux_space::model::SpaceRecord {
                    id: "space-1".into(),
                    name: "Space 1".into(),
                    profile: "Personal".into(),
                },
            })
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_agent_page_open);

        let tab = app
            .world_mut()
            .spawn(vmux_layout::tab::Tab {
                name: "t".into(),
                startup_dir: Some(tab_dir.to_string_lossy().into()),
            })
            .id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://agent/claude/".to_string(),
            request_id: None,
        });

        app.update();

        let spawns: Vec<SpawnAgentInStackRequest> = app
            .world_mut()
            .resource_mut::<Messages<SpawnAgentInStackRequest>>()
            .drain()
            .collect();
        let _ = std::fs::remove_dir_all(&space_dir);
        let _ = std::fs::remove_dir_all(&tab_dir);
        assert_eq!(spawns.len(), 1);
        assert_eq!(
            spawns[0].cwd, tab_dir,
            "claude page cwd resolves to ancestor tab startup_dir"
        );
    }

    #[test]
    fn bare_agent_open_skips_when_stack_already_has_same_agent() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_agent_page_open);

        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        // Stack already hosts a live vibe agent.
        app.world_mut().spawn((
            ChildOf(stack),
            vmux_core::agent::AgentSession {
                kind: AgentKind::Vibe,
            },
        ));
        app.world_mut().spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://agent/vibe/".to_string(),
            request_id: None,
        });

        app.update();

        let spawns: Vec<SpawnAgentInStackRequest> = app
            .world_mut()
            .resource_mut::<Messages<SpawnAgentInStackRequest>>()
            .drain()
            .collect();
        assert_eq!(
            spawns.len(),
            0,
            "bare agent open must not spawn a second agent when the stack already has one"
        );
    }

    #[test]
    fn run_terminal_cwd_inherits_agent_launch_dir() {
        let dir = std::env::temp_dir().join(format!("vmux-run-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let got = run_terminal_cwd(Some(&dir.to_string_lossy()), None);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(got, dir);
    }

    #[test]
    fn run_terminal_cwd_falls_back_when_agent_cwd_missing() {
        assert_eq!(run_terminal_cwd(Some(""), None), default_space_dir());
        assert_eq!(run_terminal_cwd(None, None), default_space_dir());
    }

    #[test]
    fn command_with_marker_is_shell_aware() {
        // The completion marker is an invisible OSC escape
        // (ESC ] 6973 ; token ; exit BEL), consumed by the terminal parser so it
        // never renders. nushell aborts `;` on failure, so it wraps in try/catch
        // and reads the exit code from the caught error.
        assert_eq!(
            command_with_marker("/opt/homebrew/bin/nu", "ls", "abc"),
            "$env.GIT_PAGER = \"cat\"; $env.PAGER = \"cat\"; $env.LESS = \"FRX\"; try { ls; print -rn $\"\\u{1b}]6973;abc;($env.LAST_EXIT_CODE)\\u{7}\" } catch { |e| print -rn $\"\\u{1b}]6973;abc;($e.exit_code? | default 1)\\u{7}\" }"
        );
        assert_eq!(
            command_with_marker("/usr/local/bin/fish", "ls", "abc"),
            "set -gx GIT_PAGER cat; set -gx PAGER cat; set -gx LESS FRX; ls; set __vmux_status $status; printf '\\033]6973;abc;%s\\007' $__vmux_status"
        );
        assert_eq!(
            command_with_marker("/bin/zsh", "ls", "abc"),
            "export GIT_PAGER=cat PAGER=cat LESS=FRX; ls; __vmux_status=\"$?\"; printf '\\033]6973;abc;%s\\007' \"$__vmux_status\""
        );
        // Unknown shells fall back to posix syntax.
        assert_eq!(
            command_with_marker("/bin/bash", "ls", "abc"),
            "export GIT_PAGER=cat PAGER=cat LESS=FRX; ls; __vmux_status=\"$?\"; printf '\\033]6973;abc;%s\\007' \"$__vmux_status\""
        );
    }

    #[test]
    fn run_command_line_noop_when_token_absent() {
        let settings = test_settings();
        assert_eq!(run_command_line("ls -la", None, &settings), "ls -la");
    }

    #[test]
    fn run_command_line_embeds_marker_when_token_present() {
        let settings = test_settings();
        let out = run_command_line("ls -la", Some("tok9"), &settings);
        assert!(out.contains("ls -la"), "got: {out}");
        assert!(out.contains("]6973;tok9;"), "got: {out}");
        assert!(
            !out.contains("__VMUX_DONE_"),
            "marker must be invisible: {out}"
        );
    }

    #[test]
    fn agent_origin_clears_requested_focus() {
        let origin = CommandOrigin::Agent {
            sid: Some("s1".into()),
            anchor: Some(ProcessId::new()),
        };

        assert!(!requested_focus_for_origin(&origin, true));
        assert!(!requested_focus_for_origin(&origin, false));
    }

    #[test]
    fn user_origin_keeps_requested_focus() {
        assert!(requested_focus_for_origin(&CommandOrigin::User, true));
        assert!(!requested_focus_for_origin(&CommandOrigin::User, false));
    }

    #[test]
    fn agent_layout_snapshot_keeps_current_focus() {
        use vmux_service::protocol::layout::{Focus, LayoutNode, LayoutSnapshot, Tab};
        let mut snapshot = LayoutSnapshot {
            tabs: vec![
                Tab {
                    id: Some("tab:9".into()),
                    name: "Agent".into(),
                    is_active: true,
                    root: LayoutNode::Pane {
                        id: Some("pane:8".into()),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                },
                Tab {
                    id: Some("tab:1".into()),
                    name: "User".into(),
                    is_active: false,
                    root: LayoutNode::Pane {
                        id: Some("pane:2".into()),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                },
            ],
            focused: Focus {
                tab: Some("tab:9".into()),
                pane: Some("pane:8".into()),
                stack: None,
            },
        };
        let focus = FocusedStack {
            tab: Some(Entity::from_bits(1)),
            pane: Some(Entity::from_bits(2)),
            stack: Some(Entity::from_bits(3)),
        };

        preserve_current_focus_in_layout_snapshot(&mut snapshot, &focus);

        assert_eq!(snapshot.focused.tab.as_deref(), Some("tab:1"));
        assert_eq!(snapshot.focused.pane.as_deref(), Some("pane:2"));
        assert_eq!(snapshot.focused.stack.as_deref(), Some("stack:3"));
        assert!(!snapshot.tabs[0].is_active);
        assert!(snapshot.tabs[1].is_active);
    }

    #[test]
    fn agent_app_command_filter_blocks_focus_changers() {
        assert!(!agent_may_dispatch_app_command(&AppCommand::Browser(
            vmux_command::BrowserCommand::Open(vmux_command::OpenCommand::InNewStack { url: None }),
        )));
        assert!(!agent_may_dispatch_app_command(&AppCommand::Browser(
            vmux_command::BrowserCommand::Bar(vmux_command::BrowserBarCommand::OpenCommandBar),
        )));
        assert!(!agent_may_dispatch_app_command(&AppCommand::Terminal(
            vmux_command::TerminalCommand::Next,
        )));
        assert!(agent_may_dispatch_app_command(&AppCommand::Terminal(
            vmux_command::TerminalCommand::Clear,
        )));
    }

    #[test]
    fn agent_run_spawns_terminal_before_next_agent_command_frame() {
        let source = include_str!("plugin.rs");
        let non_test_source = source
            .split("#[cfg(test)]")
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

    #[derive(Resource)]
    struct RunTerminalCandidateInput {
        agent_pane: Entity,
    }

    #[derive(Resource, Default)]
    struct RunTerminalCandidateOutput(Vec<RunTerminalCandidate>);

    fn collect_run_terminal_candidates(
        input: Res<RunTerminalCandidateInput>,
        terminals: Query<
            (Entity, &ProcessId),
            (
                With<Terminal>,
                Without<AgentSession>,
                Without<ProcessExited>,
            ),
        >,
        child_of_q: Query<&ChildOf>,
        tab_q: Query<Entity, With<vmux_layout::tab::Tab>>,
        seq_q: Query<&vmux_layout::pane::SpawnSeq>,
        mut out: ResMut<RunTerminalCandidateOutput>,
    ) {
        out.0 = run_terminal_candidates(input.agent_pane, &terminals, &child_of_q, &tab_q, &seq_q);
    }

    #[test]
    fn run_terminal_candidates_fail_closed_when_agent_tab_missing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalCandidateOutput>()
            .add_systems(Update, collect_run_terminal_candidates);

        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let terminal_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(7), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(terminal_pane)))
            .id();
        app.world_mut()
            .spawn((Terminal, ProcessId::new(), ChildOf(stack)));
        let agent_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(9)))
            .id();

        app.insert_resource(RunTerminalCandidateInput { agent_pane });
        app.update();

        assert!(
            app.world()
                .resource::<RunTerminalCandidateOutput>()
                .0
                .is_empty(),
            "unresolved agent tab must not match terminals from other tabs"
        );
    }

    #[derive(Resource)]
    struct RunTerminalBucketPaneInput {
        agent_pane: Entity,
    }

    #[derive(Resource, Default)]
    struct RunTerminalBucketPaneOutput(Vec<Entity>);

    fn collect_run_terminal_bucket_panes(
        input: Res<RunTerminalBucketPaneInput>,
        child_of_q: Query<&ChildOf>,
        tab_q: Query<Entity, With<vmux_layout::tab::Tab>>,
        leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
        pane_children: Query<&Children, With<Pane>>,
        stack_q: Query<Entity, With<vmux_layout::stack::Stack>>,
        page_q: Query<&PageMetadata, With<vmux_layout::stack::Stack>>,
        seq_q: Query<&vmux_layout::pane::SpawnSeq>,
        mut out: ResMut<RunTerminalBucketPaneOutput>,
    ) {
        out.0 = run_terminal_bucket_panes(
            input.agent_pane,
            &child_of_q,
            &tab_q,
            &leaf_panes,
            &pane_children,
            &stack_q,
            &page_q,
            &seq_q,
        )
        .into_iter()
        .map(|candidate| candidate.pane)
        .collect();
    }

    #[test]
    fn run_terminal_bucket_panes_include_pure_terminal_layout_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalBucketPaneOutput>()
            .add_systems(Update, collect_run_terminal_bucket_panes);

        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let agent_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(1), ChildOf(tab)))
            .id();
        let terminal_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(3), ChildOf(tab)))
            .id();
        spawn_stack_in_pane(&mut app, terminal_pane, "vmux://terminal/68001");
        let file_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(9), ChildOf(tab)))
            .id();
        spawn_stack_in_pane(&mut app, file_pane, "file:///repo/src/plugin.rs");

        app.insert_resource(RunTerminalBucketPaneInput { agent_pane });
        app.update();

        assert_eq!(
            app.world().resource::<RunTerminalBucketPaneOutput>().0,
            vec![terminal_pane]
        );
    }

    #[test]
    fn pending_run_terminal_spawn_appends_same_frame_input() {
        let anchor = ProcessId::new();
        let terminal = ProcessId::new();
        let pane = Entity::from_bits(20);
        let mut pending_spawns = std::collections::HashMap::new();
        pending_spawns.insert(
            anchor,
            PendingRunTerminalSpawn {
                pid: terminal,
                request_index: 0,
            },
        );
        let mut terminal_spawns = vec![TerminalStackSpawnRequest {
            pane,
            cwd: None,
            pending_input: Some(b"one\r".to_vec()),
            process_id: Some(terminal),
            activate: false,
        }];

        let picked = append_pending_run_terminal_input(
            anchor,
            &pending_spawns,
            &mut terminal_spawns,
            b"two\r",
        );

        assert_eq!(picked, Some(terminal));
        assert_eq!(
            terminal_spawns[0].pending_input.as_deref(),
            Some(&b"one\rtwo\r"[..])
        );
        assert_eq!(terminal_spawns.len(), 1);
    }

    #[derive(Resource)]
    struct ReusedRunPaneTouchInput {
        pane: Entity,
    }

    fn touch_reused_run_pane_spawn_seq_test_system(
        input: Res<ReusedRunPaneTouchInput>,
        mut commands: Commands,
        mut spawn_counter: ResMut<vmux_layout::pane::SpawnCounter>,
        seq_q: Query<&vmux_layout::pane::SpawnSeq>,
    ) {
        touch_reused_run_pane_spawn_seq(input.pane, &mut commands, &mut spawn_counter, &seq_q);
    }

    #[test]
    fn reusable_run_pane_touch_refreshes_spawn_seq() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<vmux_layout::pane::SpawnCounter>()
            .add_systems(Update, touch_reused_run_pane_spawn_seq_test_system);

        let reused = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(2)))
            .id();
        app.world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(10)));
        app.insert_resource(ReusedRunPaneTouchInput { pane: reused });
        app.update();

        assert_eq!(
            app.world()
                .get::<vmux_layout::pane::SpawnSeq>(reused)
                .unwrap()
                .0,
            11
        );
    }

    #[derive(Resource)]
    struct SplitRunPaneInput {
        pane: Entity,
    }

    #[derive(Resource, Default)]
    struct SplitRunPaneOutput(Option<Entity>);

    fn split_run_pane_test_system(
        input: Res<SplitRunPaneInput>,
        mut out: ResMut<SplitRunPaneOutput>,
        mut commands: Commands,
        mut spawn_counter: ResMut<vmux_layout::pane::SpawnCounter>,
        pane_children: Query<&Children, With<Pane>>,
        tab_filter: Query<Entity, With<vmux_layout::stack::Stack>>,
        split_dir_q: Query<&PaneSplit>,
        seq_q: Query<&vmux_layout::pane::SpawnSeq>,
    ) {
        let mut split_batch = std::collections::HashSet::new();
        let target = split_pane_off(
            &mut commands,
            input.pane,
            &vmux_service::protocol::AgentPaneDirection::Bottom,
            false,
            &pane_children,
            &tab_filter,
            &split_dir_q,
            &mut split_batch,
        );
        touch_reused_run_pane_spawn_seq(target, &mut commands, &mut spawn_counter, &seq_q);
        out.0 = Some(target);
    }

    #[test]
    fn split_run_pane_becomes_newest_for_followup_placement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<vmux_layout::pane::SpawnCounter>()
            .init_resource::<SplitRunPaneOutput>()
            .add_systems(Update, split_run_pane_test_system);

        let tab = app
            .world_mut()
            .spawn((vmux_layout::tab::Tab::default(), LastActivatedAt(1)))
            .id();
        let browser_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(10), ChildOf(tab)))
            .id();
        let browser_stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(browser_pane)))
            .id();
        app.world_mut()
            .entity_mut(browser_stack)
            .insert(PageMetadata {
                url: "https://news.ycombinator.com".into(),
                ..default()
            });
        app.insert_resource(SplitRunPaneInput { pane: browser_pane });

        app.update();

        let terminal_pane = app.world().resource::<SplitRunPaneOutput>().0.unwrap();
        let seq = app
            .world()
            .get::<vmux_layout::pane::SpawnSeq>(terminal_pane)
            .expect("split run target gets fresh spawn seq")
            .0;
        assert!(seq > 10, "split run target must become newest");
    }

    #[derive(Resource)]
    struct BrowserPaneClaimInput {
        anchor: ProcessId,
    }

    #[derive(Resource, Default)]
    struct BrowserPaneClaimOutput(Option<Entity>);

    fn claim_browser_pane_test_system(
        input: Res<BrowserPaneClaimInput>,
        mut resolve: AgentBrowserResolve,
        mut out: ResMut<BrowserPaneClaimOutput>,
    ) {
        out.0 = resolve.claim_browser_pane(input.anchor);
    }

    fn spawn_stack_in_pane(app: &mut App, pane: Entity, url: &str) -> Entity {
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
            .id();
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            url: url.to_string(),
            ..default()
        });
        stack
    }

    fn browser_claim_app() -> (App, ProcessId, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_layout::active_panes::ActivatePane>()
            .init_resource::<BrowserPaneClaimOutput>()
            .add_systems(Update, claim_browser_pane_test_system);
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: vmux_layout::pane::PaneSplitDirection::Row,
                },
            ))
            .id();
        let agent_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
        let agent_stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(agent_pane)))
            .id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((
            Terminal,
            anchor,
            AgentSession {
                kind: AgentKind::Codex,
            },
            ChildOf(agent_stack),
        ));
        app.insert_resource(BrowserPaneClaimInput { anchor });
        (app, anchor, split)
    }

    #[test]
    fn browser_pane_claim_ignores_mixed_file_browser_pane() {
        let (mut app, _anchor, split) = browser_claim_app();
        let mixed_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
        spawn_stack_in_pane(&mut app, mixed_pane, "file:///repo/src/main.rs");
        let browser_stack = spawn_stack_in_pane(&mut app, mixed_pane, "https://example.com");
        app.world_mut()
            .entity_mut(browser_stack)
            .insert(vmux_layout::Browser);

        app.update();

        assert_eq!(app.world().resource::<BrowserPaneClaimOutput>().0, None);
    }

    #[test]
    fn browser_pane_claim_prefers_pure_browser_pane_over_mixed_pane() {
        let (mut app, _anchor, split) = browser_claim_app();
        let mixed_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
        spawn_stack_in_pane(&mut app, mixed_pane, "file:///repo/src/main.rs");
        let mixed_browser = spawn_stack_in_pane(&mut app, mixed_pane, "https://mixed.example");
        app.world_mut()
            .entity_mut(mixed_browser)
            .insert(vmux_layout::Browser);
        let pure_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
        let pure_browser = spawn_stack_in_pane(&mut app, pure_pane, "https://pure.example");
        app.world_mut()
            .entity_mut(pure_browser)
            .insert(vmux_layout::Browser);

        app.update();

        assert_eq!(
            app.world().resource::<BrowserPaneClaimOutput>().0,
            Some(pure_pane)
        );
    }

    #[test]
    fn run_reuses_existing_terminal_when_region_cache_is_empty() {
        let anchor = ProcessId::new();
        let terminal = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let terminal_pane = Entity::from_bits(20);
        let regions = AgentTerminalRegions::default();
        let candidates = [RunTerminalCandidate {
            pid: terminal,
            stack: Entity::from_bits(21),
            pane: terminal_pane,
            pane_spawn_seq: 7,
        }];

        let picked =
            choose_reusable_run_terminal(anchor, agent_pane, &regions, &candidates).unwrap();

        assert_eq!(picked.pid, terminal);
        assert_eq!(picked.pane, terminal_pane);
    }

    #[test]
    fn run_reuses_cached_terminal_before_newer_terminal_candidates() {
        let anchor = ProcessId::new();
        let cached = ProcessId::new();
        let newer = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let cached_pane = Entity::from_bits(20);
        let newer_pane = Entity::from_bits(30);
        let mut regions = AgentTerminalRegions::default();
        regions.run_terminals.insert(anchor, cached);
        regions.run_panes.insert(anchor, cached_pane);
        let candidates = [
            RunTerminalCandidate {
                pid: cached,
                stack: Entity::from_bits(21),
                pane: cached_pane,
                pane_spawn_seq: 3,
            },
            RunTerminalCandidate {
                pid: newer,
                stack: Entity::from_bits(31),
                pane: newer_pane,
                pane_spawn_seq: 9,
            },
        ];

        let picked =
            choose_reusable_run_terminal(anchor, agent_pane, &regions, &candidates).unwrap();

        assert_eq!(picked.pid, cached);
        assert_eq!(picked.pane, cached_pane);
    }

    #[derive(Resource)]
    struct ReusedRunTerminalFocusInput {
        candidate: RunTerminalCandidate,
    }

    fn focus_reused_run_terminal_test_system(
        input: Res<ReusedRunTerminalFocusInput>,
        mut commands: Commands,
        child_of_q: Query<&ChildOf>,
        tab_q: Query<Entity, With<vmux_layout::tab::Tab>>,
    ) {
        focus_reused_run_terminal(input.candidate, &mut commands, &child_of_q, &tab_q);
    }

    #[test]
    fn reused_run_terminal_focus_activates_stack_pane_and_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, focus_reused_run_terminal_test_system);
        let tab = app
            .world_mut()
            .spawn((vmux_layout::tab::Tab::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((
                Pane,
                vmux_layout::pane::SpawnSeq(7),
                LastActivatedAt(2),
                ChildOf(tab),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::stack_bundle(),
                LastActivatedAt(3),
                ChildOf(pane),
            ))
            .id();
        app.insert_resource(ReusedRunTerminalFocusInput {
            candidate: RunTerminalCandidate {
                pid: ProcessId::new(),
                stack,
                pane,
                pane_spawn_seq: 7,
            },
        });

        app.update();

        assert!(app.world().get::<LastActivatedAt>(tab).unwrap().0 > 1);
        assert!(app.world().get::<LastActivatedAt>(pane).unwrap().0 > 2);
        assert!(app.world().get::<LastActivatedAt>(stack).unwrap().0 > 3);
    }

    #[test]
    fn split_run_stacks_into_cached_terminal_bucket_pane() {
        let anchor = ProcessId::new();
        let terminal = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let terminal_pane = Entity::from_bits(20);
        let mut regions = AgentTerminalRegions::default();
        regions.run_panes.insert(anchor, terminal_pane);
        let candidates = [RunTerminalCandidate {
            pid: terminal,
            stack: Entity::from_bits(21),
            pane: terminal_pane,
            pane_spawn_seq: 7,
        }];

        assert_eq!(
            choose_run_terminal_bucket_pane(anchor, agent_pane, &regions, &candidates),
            Some(terminal_pane)
        );
    }

    #[test]
    fn split_run_keeps_cached_terminal_bucket_after_process_exits() {
        let anchor = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let terminal_pane = Entity::from_bits(20);
        let mut regions = AgentTerminalRegions::default();
        regions.run_panes.insert(anchor, terminal_pane);
        let candidates = [];

        assert_eq!(
            choose_run_terminal_bucket_pane(anchor, agent_pane, &regions, &candidates),
            Some(terminal_pane)
        );
    }
}
