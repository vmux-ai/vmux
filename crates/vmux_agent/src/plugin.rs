use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::{CefKeyboardTarget, WebviewExtendStandardMaterial};
use vmux_command::{AppCommand, WriteAppCommands};
use vmux_core::agent::{
    AgentKind, AgentProviderTargetKind, McpServerConfig, PageAgentAttachDefaultRequest,
    PageAgentAttachRequest, PageAgentSpawnDefaultRequest, PageAgentSpawnStackRequest,
    RestartAgentPty, SpawnAgentInStackRequest,
};
use vmux_core::{
    LastActivatedAt, PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask, Ready,
};
use vmux_layout::event::TERMINAL_PAGE_URL;
use vmux_layout::{
    pane::{Pane, PaneSplit},
    stack::FocusedStack,
};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{
    AgentCommand as ServiceAgentCommand, AgentCommandResult, AgentQuery, AgentQueryResult,
    AgentRequestId, AgentShellMode, ClientMessage, ProcessId,
};
use vmux_setting::AppSettings;
use vmux_space::ActiveSpace;
use vmux_terminal::ProcessExited;
use vmux_terminal::launch::TerminalLaunch;
use vmux_terminal::{ServiceMessageSet, TerminalStackSpawnRequest, new_terminal_bundle_with_cwd};

use crate::AgentVariant;
use crate::client::cli::claude::ClaudeStrategy;
use crate::client::cli::codex::CodexStrategy;
use crate::client::cli::vibe::VibeStrategy;
use crate::events::{AgentCommandRequest, AgentQueryRequest};
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

pub struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(VibeStrategy));
        strategies.register_cli(Box::new(ClaudeStrategy));
        strategies.register_cli(Box::new(CodexStrategy));

        app.insert_resource(strategies)
            .init_resource::<AgentSessionToEntity>()
            .init_resource::<AgentSessionDirty>()
            .add_message::<AgentCommandRequest>()
            .add_message::<AgentQueryRequest>()
            .add_message::<AgentSessionExited>()
            .add_message::<SpawnAgentInStackRequest>()
            .add_message::<PageAgentAttachRequest>()
            .add_message::<PageAgentSpawnStackRequest>()
            .add_message::<PageAgentSpawnDefaultRequest>()
            .add_message::<PageAgentAttachDefaultRequest>()
            .add_message::<TerminalStackSpawnRequest>()
            .add_message::<ProcessStackSpawnRequest>()
            .add_message::<RestartAgentPty>()
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
                    handle_agent_commands,
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
                ),
            )
            .add_systems(
                Update,
                (
                    handle_spawn_agent_requests,
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
    ));
    let placeholder = page_agent_placeholder_url(provider, model, sid);
    commands.spawn((
        vmux_layout::Browser::new(meshes, webview_mt, &placeholder),
        ChildOf(stack),
    ));
    Some(())
}

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
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct AgentLookups<'w> {
    pub pid_to_entity: Option<Res<'w, vmux_terminal::pid::PidToEntity>>,
    pub agent_to_entity: Option<Res<'w, crate::session::AgentSessionToEntity>>,
    pub active_space: Option<Res<'w, ActiveSpace>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct AgentSpaceWriters<'w> {
    layout_apply: MessageWriter<'w, vmux_layout::reconcile::LayoutApplyRequest>,
    space_command: MessageWriter<'w, vmux_space::SpaceCommandRequest>,
}

fn handle_agent_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    mut app_commands: MessageWriter<AppCommand>,
    mut browser_nav_writer: MessageWriter<vmux_layout::BrowserNavigateRequest>,
    mut browser_go_back_writer: MessageWriter<vmux_layout::BrowserGoBackRequest>,
    mut browser_go_forward_writer: MessageWriter<vmux_layout::BrowserGoForwardRequest>,
    mut open_in_new_stack_writer: MessageWriter<vmux_layout::OpenInNewStackRequest>,
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
        let result = match &request.command {
            ServiceAgentCommand::AppCommand { id, args_json } => {
                let args: serde_json::Value = if args_json.is_empty() {
                    serde_json::json!({})
                } else {
                    serde_json::from_str(args_json).unwrap_or(serde_json::json!({}))
                };
                match AppCommand::from_mcp_call(id, args) {
                    Some(Ok(command)) => {
                        app_commands.write(command);
                        AgentCommandResult::Ok
                    }
                    Some(Err(message)) => AgentCommandResult::Error(message),
                    None => match AppCommand::from_mcp_id(id) {
                        Some(command) => {
                            app_commands.write(command);
                            AgentCommandResult::Ok
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
                            });
                        } else {
                            process_stack_spawn_writer.write(ProcessStackSpawnRequest {
                                pane,
                                command: command.clone(),
                                args: args.clone(),
                                cwd: cwd_path,
                                env: env.clone(),
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
                browser_nav_writer.write(vmux_layout::BrowserNavigateRequest {
                    url: url.clone(),
                    pane: pane.clone(),
                    request_id: Some(request.request_id.0),
                });
                continue;
            }
            ServiceAgentCommand::TerminalSend { text, terminal } => {
                terminal_send_writer.write(vmux_terminal::TerminalSendRequest {
                    text: text.clone(),
                    terminal: terminal.clone(),
                });
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
                writers
                    .layout_apply
                    .write(vmux_layout::reconcile::LayoutApplyRequest {
                        request_id: request.request_id.0,
                        snapshot: layout.clone(),
                    });
                continue;
            }
            ServiceAgentCommand::BrowserGoBack { pane } => {
                browser_go_back_writer
                    .write(vmux_layout::BrowserGoBackRequest { pane: pane.clone() });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::BrowserGoForward { pane } => {
                browser_go_forward_writer
                    .write(vmux_layout::BrowserGoForwardRequest { pane: pane.clone() });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::BrowserHistorySearch { query, limit } => {
                bevy::log::info!("browser_history_search: query={:?} limit={}", query, limit);
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::OpenInNewStack { url } => {
                open_in_new_stack_writer
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
        };
        if let Some(service) = service.as_ref() {
            service.0.send(ClientMessage::AgentCommandResponse {
                request_id: request.request_id,
                result,
            });
        }
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
        let stack = commands
            .spawn((
                vmux_layout::stack::stack_bundle(),
                LastActivatedAt::now(),
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
) {
    for (entity, pid, mut meta) in &mut q {
        commands
            .entity(entity)
            .remove::<AgentSession>()
            .remove::<SessionId>()
            .remove::<PendingAgentSession>();
        let next = match pid {
            Some(vmux_terminal::pid::Pid(p)) => {
                format!("{}{p}", vmux_terminal::event::TERMINAL_PAGE_URL)
            }
            None => vmux_terminal::event::TERMINAL_PAGE_URL.to_string(),
        };
        if meta.url != next {
            meta.url = next;
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
            command,
        });
    }
}

fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    settings: Res<AppSettings>,
    active_space: Option<Res<ActiveSpace>>,
    mut layout_snapshot_writer: MessageWriter<vmux_layout::reconcile::LayoutSnapshotRequest>,
) {
    let Some(service) = service else { return };

    for request in reader.read() {
        match request.query {
            AgentQuery::ReadLayout => {
                layout_snapshot_writer.write(vmux_layout::reconcile::LayoutSnapshotRequest {
                    request_id: request.request_id.0,
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
                let registry = vmux_space::spaces::read_space_registry_from(
                    &vmux_core::profile::shared_data_dir(),
                );
                let active_id = active_space.as_ref().map(|a| a.record.id.clone());
                let rows: Vec<serde_json::Value> = registry
                    .spaces
                    .iter()
                    .map(|space| {
                        serde_json::json!({
                            "id": space.id,
                            "name": space.name,
                            "profile": space.profile,
                            "is_active": active_id.as_deref() == Some(space.id.as_str()),
                        })
                    })
                    .collect();
                let json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string());
                service.0.send(ClientMessage::AgentQueryResponse {
                    request_id: request.request_id,
                    result: AgentQueryResult::Spaces(json),
                });
            }
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

fn handle_agent_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    child_of_q: Query<&ChildOf>,
    agent_to_entity: Option<Res<AgentSessionToEntity>>,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
    mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (entity, task) in &tasks {
        if !task.url.starts_with("vmux://agent/") {
            continue;
        }
        match handle_agent_page_open_task(
            task,
            &children_q,
            &child_of_q,
            agent_to_entity.as_deref(),
            idx.as_deref(),
            &kind_q,
            &mut spawn_agent,
            &mut commands,
            &mut meshes,
            &mut webview_mt,
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
    child_of_q: &Query<&ChildOf>,
    agent_to_entity: Option<&AgentSessionToEntity>,
    idx: Option<&crate::client::page::strategy_index::PageStrategyIndex>,
    kind_q: &Query<&crate::client::page::strategy_components::StrategyKind>,
    spawn_agent: &mut MessageWriter<SpawnAgentInStackRequest>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Result<(), String> {
    // The Vibe-CLI setup page lives in the agent namespace but is a *served* page, not an agent
    // session — attach it directly rather than mis-parsing "vibe/setup" as a Page-agent provider/model.
    if task.url == "vmux://agent/vibe/setup" {
        attach_vibe_cli_setup_to_stack(task.stack, children_q, commands, meshes, webview_mt);
        return Ok(());
    }
    let parsed =
        url::Url::parse(&task.url).map_err(|e| format!("invalid agent URL '{}': {e}", task.url))?;
    let path = parsed.path().trim_start_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
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
            if let Some(map) = agent_to_entity
                && let Some(&entity) = map.0.get(&(kind, sid.clone()))
            {
                vmux_terminal::pid::focus_pane_entity(entity, commands, child_of_q);
                return Ok(());
            }
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
            spawn_agent.write(SpawnAgentInStackRequest {
                kind,
                cwd,
                session_id: Some(sid),
                stack: task.stack,
            });
            Ok(())
        }
        None => {
            if segs.len() == 1
                && let Some(kind) = AgentKind::from_url_segment(segs[0])
            {
                let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
                spawn_agent.write(SpawnAgentInStackRequest {
                    kind,
                    cwd,
                    session_id: None,
                    stack: task.stack,
                });
                return Ok(());
            }
            if segs.len() == 2 {
                let provider = segs[0];
                let model = segs[1];
                let idx = idx.ok_or_else(|| "page strategy index not registered".to_string())?;
                let sid = uuid::Uuid::new_v4().to_string();
                clear_stack_children(task.stack, children_q, commands);
                attach_page_agent_to_stack(
                    task.stack, provider, model, &sid, commands, meshes, webview_mt, idx, kind_q,
                )
                .ok_or_else(|| {
                    format!("no Page agent strategy registered for {provider}/{model}")
                })?;
                return Ok(());
            }
            Err(format!("malformed agent URL '{}'", task.url))
        }
    }
}

fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
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
    if kind == AgentKind::Vibe && message == "vibe executable not found" {
        attach_vibe_cli_setup_to_stack(stack, children_q, commands, meshes, webview_mt);
        return;
    }

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

fn attach_vibe_cli_setup_to_stack(
    stack: Entity,
    children_q: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    clear_stack_children(stack, children_q, commands);
    let title = "Set up Vibe CLI";
    let url = "vmux://agent/vibe/setup";
    commands.entity(stack).insert(PageMetadata {
        url: url.to_string(),
        title: title.to_string(),
        bg_color: Some("#101114".to_string()),
        ..default()
    });
    let browser = commands
        .spawn((
            vmux_layout::Browser::new_with_title(meshes, webview_mt, url, title),
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
            let message = format!("{} executable not found", req.kind.executable());
            bevy::log::warn!("agent spawn ({:?}) failed: {message}", req.kind);
            attach_agent_spawn_error_to_stack(
                req.stack,
                req.kind,
                &message,
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            continue;
        };
        match crate::build_agent_launch(
            req.kind,
            &req.cwd,
            req.session_id.as_deref(),
            strategies,
            &exe_path,
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
                commands
                    .entity(terminal)
                    .insert(CefKeyboardTarget)
                    .insert((launch, AgentSession { kind: req.kind }));
                if let Some(id) = req.session_id.clone() {
                    commands.entity(terminal).insert(SessionId(id));
                } else {
                    commands.entity(terminal).insert(PendingAgentSession {
                        kind: req.kind,
                        spawn_time: std::time::SystemTime::now(),
                        cwd: req.cwd.clone(),
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

fn handle_restart_agent_pty(
    mut reader: MessageReader<RestartAgentPty>,
    mut q: Query<(
        &mut ProcessId,
        Option<&mut TerminalLaunch>,
        &AgentSession,
        Option<&SessionId>,
    )>,
    service: Option<Res<ServiceClient>>,
    strategies: Option<Res<AgentStrategies>>,
) {
    let Some(service) = service else {
        for _ in reader.read() {}
        return;
    };
    for msg in reader.read() {
        let Ok((mut pid, mut launch, session, session_id)) = q.get_mut(msg.entity) else {
            continue;
        };
        service
            .0
            .send(ClientMessage::KillProcess { process_id: *pid });

        let (command, args, cwd, env) = match launch.as_deref() {
            Some(l) => {
                let mut updated_args = l.args.clone();
                if let Some(strats) = strategies.as_deref()
                    && let Some(strategy) = strats.get_cli(session.kind)
                {
                    let mcp = McpServerConfig {
                        command: l.command.clone(),
                        args: vec![],
                        cwd: None,
                    };
                    updated_args = strategy.build_args(&mcp, session_id.map(|s| s.0.as_str()));
                }
                (
                    l.command.clone(),
                    updated_args,
                    l.cwd.clone(),
                    l.env.clone(),
                )
            }
            None => (String::new(), vec![], String::new(), Vec::new()),
        };

        let new_id = ProcessId::new();
        service.0.send(ClientMessage::CreateProcess {
            process_id: new_id,
            command: command.clone(),
            args: args.clone(),
            cwd: cwd.clone(),
            env: env.clone(),
            cols: 80,
            rows: 24,
        });
        service
            .0
            .send(ClientMessage::AttachProcess { process_id: new_id });

        *pid = new_id;
        if let Some(l) = launch.as_mut() {
            l.args = args;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{BrowserSettings, ShortcutSettings};
    use vmux_terminal::Terminal;

    pub(super) fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
        }
    }

    #[test]
    fn blank_cwd_is_accepted() {
        assert_eq!(valid_cwd("").unwrap(), None);
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
        let terminal = app.world_mut().spawn(Terminal).insert(ChildOf(stack)).id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
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
}
