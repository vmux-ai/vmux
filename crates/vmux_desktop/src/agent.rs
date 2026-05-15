use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    command::{AppCommand, WriteAppCommands},
    layout::{
        pane::{Pane, PaneSplit},
        stack::FocusedStack,
    },
    settings::AppSettings,
    terminal::{PendingTerminalInput, ProcessExited, ServiceMessageSet, Terminal},
};
use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::{CefKeyboardTarget, RequestNavigate, WebviewExtendStandardMaterial};

use crate::browser::Browser;
use vmux_agent::session::{AgentSession, PendingAgentSession, SessionId};
use vmux_agent::strategy::AgentStrategies;
use vmux_agent::{AgentKind, mcp};
use vmux_core::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_layout::event::TERMINAL_WEBVIEW_URL;
use vmux_service::protocol::{AgentCommand as ServiceAgentCommand, AgentRequestId, AgentShellMode};

#[derive(Message)]
pub(crate) struct AgentCommandRequest {
    pub(crate) request_id: AgentRequestId,
    pub(crate) command: ServiceAgentCommand,
}

#[derive(Message)]
pub(crate) struct AgentQueryRequest {
    pub(crate) request_id: AgentRequestId,
    pub(crate) query: vmux_service::protocol::AgentQuery,
}

#[derive(Clone)]
pub(crate) struct AgentProvider {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) shortcut: &'static str,
    pub(crate) executable: &'static str,
    pub(crate) available: fn() -> bool,
    pub(crate) prepare: fn(&Path) -> Result<PreparedAgentLaunch, String>,
}

pub(crate) struct PreparedAgentLaunch {
    pub(crate) kind: AgentKind,
    pub(crate) cwd: PathBuf,
    pub(crate) launch: crate::terminal::launch::TerminalLaunch,
}

pub(crate) struct AgentCommandEntry {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) shortcut: &'static str,
}

#[derive(Resource, Default)]
pub(crate) struct AgentProviders {
    providers: BTreeMap<&'static str, AgentProvider>,
}

impl AgentProviders {
    #[allow(dead_code)]
    pub(crate) fn register(&mut self, provider: AgentProvider) {
        self.providers.insert(provider.id, provider);
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn get(&self, id: &str) -> Option<&AgentProvider> {
        self.providers.get(id)
    }

    pub(crate) fn command_entries(&self) -> Vec<AgentCommandEntry> {
        self.providers
            .values()
            .filter(|provider| (provider.available)())
            .map(|provider| AgentCommandEntry {
                id: provider.id,
                name: provider.name,
                shortcut: provider.shortcut,
            })
            .collect()
    }

    fn prepare(&self, id: &str, cwd: &Path) -> Result<Option<PreparedAgentLaunch>, String> {
        let Some(provider) = self.providers.get(id) else {
            return Ok(None);
        };
        if !(provider.available)() {
            return Err(format!(
                "{} executable not found: {}",
                provider.name, provider.executable
            ));
        }
        (provider.prepare)(cwd).map(Some)
    }
}

pub(crate) fn default_space_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let dir = home.join(".vmux").join("default").join("space");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[derive(Message)]
pub(crate) struct AgentLaunchRequested {
    pub(crate) provider_id: String,
    pub(crate) cwd: PathBuf,
}

fn vibe_available() -> bool {
    vmux_agent::exec::find_executable("vibe").is_some()
}

fn claude_available() -> bool {
    vmux_agent::exec::find_executable("claude").is_some()
}

fn codex_available() -> bool {
    vmux_agent::exec::find_executable("codex").is_some()
}

fn vibe_prepare(cwd: &Path) -> Result<PreparedAgentLaunch, String> {
    prepare_for_kind(AgentKind::Vibe, cwd)
}

fn claude_prepare(cwd: &Path) -> Result<PreparedAgentLaunch, String> {
    prepare_for_kind(AgentKind::Claude, cwd)
}

fn codex_prepare(cwd: &Path) -> Result<PreparedAgentLaunch, String> {
    prepare_for_kind(AgentKind::Codex, cwd)
}

fn prepare_for_kind(kind: AgentKind, cwd: &Path) -> Result<PreparedAgentLaunch, String> {
    use vmux_agent::claude::ClaudeStrategy;
    use vmux_agent::codex::CodexStrategy;
    use vmux_agent::vibe::VibeStrategy;
    let mut strategies = AgentStrategies::default();
    strategies.register(Box::new(VibeStrategy));
    strategies.register(Box::new(ClaudeStrategy));
    strategies.register(Box::new(CodexStrategy));
    let launch = build_agent_launch(kind, cwd, None, &strategies)?;
    Ok(PreparedAgentLaunch {
        kind,
        cwd: cwd.to_path_buf(),
        launch,
    })
}

pub(crate) struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AgentProviders>()
            .add_message::<AgentCommandRequest>()
            .add_message::<AgentQueryRequest>()
            .add_message::<AgentLaunchRequested>()
            .add_message::<vmux_agent::AgentSessionExited>()
            .add_message::<crate::settings::SettingsWriteRequest>()
            .add_systems(
                Update,
                (
                    handle_agent_launch_requests,
                    handle_agent_commands,
                    crate::agent_query::handle_agent_queries,
                    detect_agent_session_process_exit,
                )
                    .chain()
                    .in_set(WriteAppCommands)
                    .after(ServiceMessageSet),
            );

        let mut providers = app.world_mut().resource_mut::<AgentProviders>();
        for (id, name, exe, available, prepare) in [
            (
                "vibe_new",
                "Vibe New",
                "vibe",
                vibe_available as fn() -> bool,
                vibe_prepare as fn(&Path) -> Result<PreparedAgentLaunch, String>,
            ),
            (
                "vibe_new_stack",
                "Vibe New Stack",
                "vibe",
                vibe_available,
                vibe_prepare,
            ),
            (
                "claude_new",
                "Claude New",
                "claude",
                claude_available,
                claude_prepare,
            ),
            (
                "claude_new_stack",
                "Claude New Stack",
                "claude",
                claude_available,
                claude_prepare,
            ),
            (
                "codex_new",
                "Codex New",
                "codex",
                codex_available,
                codex_prepare,
            ),
            (
                "codex_new_stack",
                "Codex New Stack",
                "codex",
                codex_available,
                codex_prepare,
            ),
        ] {
            providers.register(AgentProvider {
                id,
                name,
                shortcut: "",
                executable: exe,
                available,
                prepare,
            });
        }
    }
}

pub(crate) fn shell_command_input(command: &str) -> Vec<u8> {
    let mut data = command.as_bytes().to_vec();
    data.push(b'\r');
    data
}

fn valid_cwd(cwd: &str) -> Result<Option<std::path::PathBuf>, String> {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let path = std::path::PathBuf::from(trimmed);
    if !path.exists() {
        return Err(format!("cwd does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("cwd is not a directory: {}", path.display()));
    }
    Ok(Some(path))
}

pub(crate) fn spawn_terminal_tab(
    pane: Entity,
    cwd: Option<&std::path::Path>,
    pending_input: Option<Vec<u8>>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Entity {
    let tab = commands
        .spawn((
            crate::layout::stack::stack_bundle(),
            LastActivatedAt::now(),
            ChildOf(pane),
        ))
        .id();
    let title = cwd
        .map(|cwd| format!("Terminal ({})", cwd.display()))
        .unwrap_or_else(|| "Terminal".to_string());
    commands.entity(tab).insert(PageMetadata {
        url: TERMINAL_WEBVIEW_URL.to_string(),
        title,
        ..default()
    });
    let terminal = commands
        .spawn((
            Terminal::new_with_cwd(meshes, webview_mt, settings, cwd),
            ChildOf(tab),
        ))
        .id();
    commands.entity(terminal).insert(CefKeyboardTarget);
    if let Some(data) = pending_input {
        commands
            .entity(terminal)
            .insert(PendingTerminalInput { data });
    }
    terminal
}

pub(crate) fn build_agent_launch(
    kind: AgentKind,
    cwd: &Path,
    session_id: Option<&str>,
    strategies: &AgentStrategies,
) -> Result<crate::terminal::launch::TerminalLaunch, String> {
    let strategy = strategies
        .get(kind)
        .ok_or_else(|| format!("strategy not registered for {:?}", kind))?;
    let exe_name = kind.executable();
    let exe_path = vmux_agent::exec::find_executable(exe_name)
        .ok_or_else(|| format!("{exe_name} executable not found"))?;
    let mcp_cfg = mcp::resolve(cwd)?;
    let args = strategy.build_args(&mcp_cfg, session_id);
    let mut env: Vec<(String, String)> = std::env::vars().collect();
    env.extend(strategy.build_env(&mcp_cfg));
    Ok(crate::terminal::launch::TerminalLaunch {
        command: exe_path.to_string_lossy().to_string(),
        args,
        cwd: cwd.to_string_lossy().to_string(),
        env,
        kind: kind.into(),
    })
}

pub(crate) fn spawn_fresh_agent_tab(
    kind: AgentKind,
    pane: Entity,
    cwd: PathBuf,
    strategies: &AgentStrategies,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Result<Entity, String> {
    let launch = build_agent_launch(kind, &cwd, None, strategies)?;
    let terminal = spawn_terminal_tab(
        pane,
        Some(&cwd),
        None,
        commands,
        meshes,
        webview_mt,
        settings,
    );
    commands.entity(terminal).insert((
        launch,
        AgentSession { kind },
        PendingAgentSession {
            kind,
            spawn_time: std::time::SystemTime::now(),
            cwd,
        },
    ));
    Ok(terminal)
}

pub(crate) fn spawn_agent_resume_tab(
    kind: AgentKind,
    pane: Entity,
    cwd: PathBuf,
    session_id: String,
    strategies: &AgentStrategies,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Result<Entity, String> {
    let launch = build_agent_launch(kind, &cwd, Some(&session_id), strategies)?;
    let terminal = spawn_terminal_tab(
        pane,
        Some(&cwd),
        None,
        commands,
        meshes,
        webview_mt,
        settings,
    );
    commands
        .entity(terminal)
        .insert((launch, AgentSession { kind }, SessionId(session_id)));
    Ok(terminal)
}

pub(crate) fn spawn_browser_tab(
    pane: Entity,
    url: &str,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    let tab = commands
        .spawn((
            crate::layout::stack::stack_bundle(),
            LastActivatedAt::now(),
            ChildOf(pane),
        ))
        .id();
    commands.entity(tab).insert(PageMetadata {
        url: url.to_string(),
        title: url.to_string(),
        ..default()
    });
    commands.spawn((
        crate::browser::Browser::new(meshes, webview_mt, url),
        ChildOf(tab),
    ));
    tab
}

pub(crate) fn spawn_sessions_tab(
    pane: Entity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    let tab = commands
        .spawn((
            crate::layout::stack::stack_bundle(),
            LastActivatedAt::now(),
            ChildOf(pane),
        ))
        .id();
    commands.entity(tab).insert(PageMetadata {
        url: vmux_space::event::SPACES_WEBVIEW_URL.to_string(),
        title: "Sessions".to_string(),
        ..default()
    });
    commands.spawn((
        crate::spaces::SpacesView::new(meshes, webview_mt),
        ChildOf(tab),
    ));
    tab
}

pub(crate) fn spawn_processes_tab(
    pane: Entity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    let tab = commands
        .spawn((
            crate::layout::stack::stack_bundle(),
            LastActivatedAt::now(),
            ChildOf(pane),
        ))
        .id();
    commands.entity(tab).insert(PageMetadata {
        url: vmux_service::webview::event::PROCESSES_WEBVIEW_URL.to_string(),
        title: "Background Services".to_string(),
        ..default()
    });
    commands.spawn((
        crate::processes_monitor::ProcessesMonitor::new(meshes, webview_mt),
        ChildOf(tab),
    ));
    tab
}

fn spawn_vmux_tab(
    url: &str,
    pane: Entity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
    pid_to_entity: Option<&crate::terminal::pid::PidToEntity>,
    agent_to_entity: Option<&vmux_agent::session::AgentSessionToEntity>,
    strategies: &AgentStrategies,
    child_of_q: &Query<&ChildOf>,
) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("invalid vmux URL '{url}': {e}"))?;
    let host = parsed.host_str().unwrap_or("");

    match host {
        "terminal" => {
            let path = parsed.path().trim_start_matches('/');
            if !path.is_empty() {
                match path.parse::<u32>() {
                    Ok(pid) => {
                        if let Some(map) = pid_to_entity
                            && let Some(&entity) = map.0.get(&pid)
                        {
                            crate::terminal::pid::focus_pane_entity(entity, commands, child_of_q);
                            return Ok(());
                        }
                        bevy::log::warn!("no terminal pane for pid {pid}; spawning new");
                    }
                    Err(_) => {
                        return Err(format!("malformed terminal URL '{url}'"));
                    }
                }
            }
            let cwd_param = parsed
                .query_pairs()
                .find(|(k, _)| k == "cwd")
                .map(|(_, v)| v.into_owned());
            let cwd_path = if let Some(c) = cwd_param.as_deref() {
                valid_cwd(c)?
            } else {
                None
            };
            spawn_terminal_tab(
                pane,
                cwd_path.as_deref(),
                None,
                commands,
                meshes,
                webview_mt,
                settings,
            );
            Ok(())
        }
        "sessions" => {
            spawn_sessions_tab(pane, commands, meshes, webview_mt);
            Ok(())
        }
        "services" => {
            spawn_processes_tab(pane, commands, meshes, webview_mt);
            Ok(())
        }
        "vibe" | "claude" | "codex" => {
            let kind = AgentKind::from_host(host).expect("matched above");
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
            let path = parsed.path().trim_start_matches('/');
            if path.is_empty() {
                if let Err(e) = spawn_fresh_agent_tab(
                    kind, pane, cwd, strategies, commands, meshes, webview_mt, settings,
                ) {
                    bevy::log::warn!(
                        "spawn_fresh_agent_tab({kind:?}) failed: {e}; falling back to terminal"
                    );
                    spawn_terminal_tab(pane, None, None, commands, meshes, webview_mt, settings);
                }
                Ok(())
            } else {
                let session_id = path.to_string();
                if let Some(map) = agent_to_entity
                    && let Some(&entity) = map.0.get(&(kind, session_id.clone()))
                {
                    crate::terminal::pid::focus_pane_entity(entity, commands, child_of_q);
                    return Ok(());
                }
                if let Err(e) = spawn_agent_resume_tab(
                    kind, pane, cwd, session_id, strategies, commands, meshes, webview_mt, settings,
                ) {
                    bevy::log::warn!(
                        "spawn_agent_resume_tab({kind:?}) failed: {e}; falling back to terminal"
                    );
                    spawn_terminal_tab(pane, None, None, commands, meshes, webview_mt, settings);
                }
                Ok(())
            }
        }
        other => Err(format!("unknown vmux URL host '{other}' in '{url}'")),
    }
}

fn active_terminal_for_tab(
    tab: Option<Entity>,
    terminals: &Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
) -> Option<Entity> {
    let tab = tab?;
    terminals
        .iter()
        .find_map(|(entity, child_of)| (child_of.get() == tab).then_some(entity))
}

fn active_webview_for_tab(
    tab: Option<Entity>,
    browsers: &Query<(Entity, &ChildOf), With<Browser>>,
    terminals: &Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
) -> Option<Entity> {
    let tab = tab?;
    browsers.iter().find_map(|(entity, child_of)| {
        if child_of.get() != tab {
            return None;
        }
        if terminals.iter().any(|(t, _)| t == entity) {
            return None;
        }
        Some(entity)
    })
}

fn parse_pane_target(
    s: &str,
    panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Option<Entity> {
    let bits = s.parse::<u64>().ok()?;
    let entity = Entity::try_from_bits(bits)?;
    panes.contains(entity).then_some(entity)
}

fn parse_terminal_target(
    s: &str,
    terminals: &Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
) -> Option<Entity> {
    let bits = s.parse::<u64>().ok()?;
    let entity = Entity::try_from_bits(bits)?;
    terminals.iter().any(|(e, _)| e == entity).then_some(entity)
}

#[derive(bevy::ecs::system::SystemParam)]
struct SpawnAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    webview_mt: ResMut<'w, Assets<WebviewExtendStandardMaterial>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct SettingsParams<'w> {
    settings: ResMut<'w, AppSettings>,
    writes: MessageWriter<'w, crate::settings::SettingsWriteRequest>,
}

fn handle_agent_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    mut app_commands: MessageWriter<AppCommand>,
    focus: Res<FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    pane_children: Query<&Children, With<Pane>>,
    tab_filter: Query<(), With<crate::layout::stack::Stack>>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    child_of_q: Query<&ChildOf>,
    pid_to_entity: Option<Res<crate::terminal::pid::PidToEntity>>,
    agent_to_entity: Option<Res<vmux_agent::session::AgentSessionToEntity>>,
    strategies: Res<AgentStrategies>,
    mut sp: SettingsParams,
    service: Option<Res<crate::terminal::ServiceClient>>,
    mut commands: Commands,
    mut assets: SpawnAssets,
) {
    use vmux_service::protocol::{AgentCommandResult, ClientMessage};

    for request in reader.read() {
        let result = match &request.command {
            ServiceAgentCommand::AppCommand { id } => {
                if let Some(command) = AppCommand::from_mcp_id(id) {
                    app_commands.write(command);
                    AgentCommandResult::Ok
                } else {
                    AgentCommandResult::Error(format!("unknown app command: {id}"))
                }
            }
            ServiceAgentCommand::NewTerminalTab { cwd } => {
                if let Some(pane) = focus.pane.filter(|pane| panes.contains(*pane)) {
                    match valid_cwd(cwd) {
                        Ok(cwd_path) => {
                            spawn_terminal_tab(
                                pane,
                                cwd_path.as_deref(),
                                None,
                                &mut commands,
                                &mut assets.meshes,
                                &mut assets.webview_mt,
                                &sp.settings,
                            );
                            AgentCommandResult::Ok
                        }
                        Err(message) => AgentCommandResult::Error(message),
                    }
                } else {
                    AgentCommandResult::Error("no active pane".to_string())
                }
            }
            ServiceAgentCommand::RunShell { command, cwd, mode } => {
                let input = shell_command_input(command);
                if matches!(mode, AgentShellMode::Active)
                    && let Some(terminal) = active_terminal_for_tab(focus.stack, &terminals)
                {
                    commands
                        .entity(terminal)
                        .insert(PendingTerminalInput { data: input });
                    AgentCommandResult::Ok
                } else if let Some(pane) = focus.pane.filter(|pane| panes.contains(*pane)) {
                    match valid_cwd(cwd) {
                        Ok(cwd_path) => {
                            spawn_terminal_tab(
                                pane,
                                cwd_path.as_deref(),
                                Some(input),
                                &mut commands,
                                &mut assets.meshes,
                                &mut assets.webview_mt,
                                &sp.settings,
                            );
                            AgentCommandResult::Ok
                        }
                        Err(message) => AgentCommandResult::Error(message),
                    }
                } else {
                    AgentCommandResult::Error("no active pane".to_string())
                }
            }
            ServiceAgentCommand::BrowserNavigate { url, pane } => {
                if url.starts_with("vmux://") {
                    let target = match pane.as_deref() {
                        Some(s) => match parse_pane_target(s, &panes) {
                            Some(t) => Some(t),
                            None => {
                                let result = AgentCommandResult::Error(format!(
                                    "browser_navigate: invalid pane id '{s}'"
                                ));
                                if let Some(service) = service.as_ref() {
                                    service.0.send(ClientMessage::AgentCommandResponse {
                                        request_id: request.request_id,
                                        result,
                                    });
                                }
                                continue;
                            }
                        },
                        None => focus.pane.filter(|p| panes.contains(*p)),
                    };

                    if let Some(pane_entity) = target {
                        match spawn_vmux_tab(
                            url,
                            pane_entity,
                            &mut commands,
                            &mut assets.meshes,
                            &mut assets.webview_mt,
                            &sp.settings,
                            pid_to_entity.as_deref(),
                            agent_to_entity.as_deref(),
                            &strategies,
                            &child_of_q,
                        ) {
                            Ok(()) => AgentCommandResult::Ok,
                            Err(message) => {
                                AgentCommandResult::Error(format!("browser_navigate: {message}"))
                            }
                        }
                    } else {
                        AgentCommandResult::Error(
                            "browser_navigate: no focused pane for vmux URL".to_string(),
                        )
                    }
                } else if let Some(s) = pane.as_deref() {
                    if let Some(target) = parse_pane_target(s, &panes) {
                        spawn_browser_tab(
                            target,
                            url,
                            &mut commands,
                            &mut assets.meshes,
                            &mut assets.webview_mt,
                        );
                        AgentCommandResult::Ok
                    } else {
                        AgentCommandResult::Error(format!(
                            "browser_navigate: invalid pane id '{s}'"
                        ))
                    }
                } else if let Some(webview) =
                    active_webview_for_tab(focus.stack, &browsers, &terminals)
                {
                    commands.trigger(RequestNavigate {
                        webview,
                        url: url.clone(),
                    });
                    AgentCommandResult::Ok
                } else if let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) {
                    spawn_browser_tab(
                        pane,
                        url,
                        &mut commands,
                        &mut assets.meshes,
                        &mut assets.webview_mt,
                    );
                    AgentCommandResult::Ok
                } else {
                    AgentCommandResult::Error("browser_navigate: no focused pane".to_string())
                }
            }
            ServiceAgentCommand::TerminalSend { text, terminal } => {
                let target = if let Some(s) = terminal.as_deref() {
                    match parse_terminal_target(s, &terminals) {
                        Some(t) => Ok(Some(t)),
                        None => Err(format!("terminal_send: invalid terminal id '{s}'")),
                    }
                } else {
                    Ok(active_terminal_for_tab(focus.stack, &terminals))
                };

                match target {
                    Err(message) => AgentCommandResult::Error(message),
                    Ok(Some(terminal_entity)) => {
                        commands
                            .entity(terminal_entity)
                            .insert(PendingTerminalInput {
                                data: text.as_bytes().to_vec(),
                            });
                        AgentCommandResult::Ok
                    }
                    Ok(None) => {
                        AgentCommandResult::Error("terminal_send: no active terminal".to_string())
                    }
                }
            }
            ServiceAgentCommand::UpdateSettings { path, value_json } => {
                match serde_json::from_str::<serde_json::Value>(value_json) {
                    Ok(value) => match crate::settings::apply_settings_update(
                        sp.settings.as_mut(),
                        path,
                        value,
                    ) {
                        Ok(ron_bytes) => {
                            sp.writes
                                .write(crate::settings::SettingsWriteRequest { ron_bytes });
                            AgentCommandResult::Ok
                        }
                        Err(message) => AgentCommandResult::Error(message),
                    },
                    Err(e) => AgentCommandResult::Error(format!(
                        "update_settings: invalid JSON value: {e}"
                    )),
                }
            }
            ServiceAgentCommand::SplitAndNavigate { direction, url } => {
                let split_dir_result = match direction.as_str() {
                    "right" => Ok(vmux_layout::pane::PaneSplitDirection::Row),
                    "down" => Ok(vmux_layout::pane::PaneSplitDirection::Column),
                    other => Err(format!("split_and_navigate: invalid direction '{other}'")),
                };

                match split_dir_result {
                    Err(message) => AgentCommandResult::Error(message),
                    Ok(split_dir) => {
                        if let Some(active_pane) = focus.pane.filter(|p| panes.contains(*p)) {
                            let existing_tabs: Vec<Entity> = pane_children
                                .get(active_pane)
                                .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                                .unwrap_or_default();

                            let (_pane1, pane2) = vmux_layout::pane::split_pane_in_two(
                                &mut commands,
                                active_pane,
                                split_dir,
                                &sp.settings.layout.pane,
                                &existing_tabs,
                            );
                            if url.starts_with("vmux://") {
                                match spawn_vmux_tab(
                                    url,
                                    pane2,
                                    &mut commands,
                                    &mut assets.meshes,
                                    &mut assets.webview_mt,
                                    &sp.settings,
                                    pid_to_entity.as_deref(),
                                    agent_to_entity.as_deref(),
                                    &strategies,
                                    &child_of_q,
                                ) {
                                    Ok(()) => AgentCommandResult::Ok,
                                    Err(message) => AgentCommandResult::Error(format!(
                                        "split_and_navigate: {message}"
                                    )),
                                }
                            } else {
                                spawn_browser_tab(
                                    pane2,
                                    url,
                                    &mut commands,
                                    &mut assets.meshes,
                                    &mut assets.webview_mt,
                                );
                                AgentCommandResult::Ok
                            }
                        } else {
                            AgentCommandResult::Error(
                                "split_and_navigate: no focused pane".to_string(),
                            )
                        }
                    }
                }
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

#[allow(clippy::type_complexity)]
pub(crate) fn detect_agent_session_process_exit(
    mut commands: Commands,
    mut writer: MessageWriter<vmux_agent::AgentSessionExited>,
    mut q: Query<
        (
            Entity,
            Option<&crate::terminal::pid::Pid>,
            &mut PageMetadata,
        ),
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
            Some(crate::terminal::pid::Pid(p)) => {
                format!("{}{p}", vmux_terminal::event::TERMINAL_WEBVIEW_URL)
            }
            None => vmux_terminal::event::TERMINAL_WEBVIEW_URL.to_string(),
        };
        if meta.url != next {
            meta.url = next;
        }
        writer.write(vmux_agent::AgentSessionExited { entity });
    }
}

fn handle_agent_launch_requests(
    mut reader: MessageReader<AgentLaunchRequested>,
    providers: Res<AgentProviders>,
    focus: Res<FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for request in reader.read() {
        let prepared = match providers.prepare(&request.provider_id, &request.cwd) {
            Ok(Some(prepared)) => prepared,
            Ok(None) => {
                warn!("unknown agent provider: {}", request.provider_id);
                continue;
            }
            Err(message) => {
                warn!("{message}");
                continue;
            }
        };
        let Some(pane) = focus.pane.filter(|pane| panes.contains(*pane)) else {
            warn!("agent launch has no active pane");
            continue;
        };
        let terminal = spawn_terminal_tab(
            pane,
            Some(&prepared.cwd),
            None,
            &mut commands,
            &mut meshes,
            &mut webview_mt,
            &settings,
        );
        commands.entity(terminal).insert((
            prepared.launch,
            AgentSession {
                kind: prepared.kind,
            },
            PendingAgentSession {
                kind: prepared.kind,
                spawn_time: std::time::SystemTime::now(),
                cwd: prepared.cwd.clone(),
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{
        BrowserSettings, FocusRingSettings, LayoutSettings, PaneSettings, ShortcutSettings,
        SideSheetSettings, WindowSettings,
    };

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings {
                    gap: 0.0,
                    radius: 0.0,
                },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            startup_url: None,
        }
    }

    #[test]
    fn shell_command_input_appends_carriage_return() {
        assert_eq!(shell_command_input("echo hi"), b"echo hi\r".to_vec());
    }

    #[test]
    fn blank_cwd_is_accepted() {
        assert_eq!(valid_cwd("").unwrap(), None);
    }

    fn fake_prepare(cwd: &std::path::Path) -> Result<PreparedAgentLaunch, String> {
        Ok(PreparedAgentLaunch {
            kind: AgentKind::Vibe,
            cwd: cwd.to_path_buf(),
            launch: crate::terminal::launch::TerminalLaunch {
                command: "echo".to_string(),
                args: vec!["agent".to_string()],
                cwd: cwd.to_string_lossy().to_string(),
                env: vec![],
                kind: crate::terminal::launch::TerminalKind::Vibe,
            },
        })
    }

    #[derive(Resource, Default)]
    struct CapturedNavigateUrls(Vec<String>);

    #[test]
    fn browser_navigate_triggers_request_navigate_with_url() {
        use crate::browser::Browser;
        use bevy_cef::prelude::RequestNavigate;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.init_resource::<CapturedNavigateUrls>();

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(crate::layout::stack::stack_bundle())
            .insert(ChildOf(pane))
            .id();
        app.world_mut().spawn(Browser).insert(ChildOf(stack));

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

        app.add_observer(
            |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                captured.0.push(trigger.url.clone());
            },
        );

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: None,
                },
            });

        app.update();

        let captured = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(captured.0, vec!["https://example.com".to_string()]);
    }

    #[test]
    fn agent_launch_request_uses_registered_provider_to_spawn_terminal_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut()
            .resource_mut::<AgentProviders>()
            .register(AgentProvider {
                id: "fake_agent",
                name: "Fake Agent",
                shortcut: "",
                executable: "fake",
                available: || true,
                prepare: fake_prepare,
            });
        let cwd = std::env::current_dir().unwrap();
        app.world_mut()
            .resource_mut::<Messages<AgentLaunchRequested>>()
            .write(AgentLaunchRequested {
                provider_id: "fake_agent".to_string(),
                cwd: cwd.clone(),
            });

        app.update();

        let mut terminals = app.world_mut().query::<(
            &Terminal,
            &crate::terminal::launch::TerminalLaunch,
            &ChildOf,
        )>();
        let rows: Vec<_> = terminals
            .iter(app.world())
            .map(|(_, launch, child_of)| (launch.command.clone(), child_of.get()))
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "echo");

        let tab = rows[0].1;
        assert!(
            app.world()
                .get::<crate::layout::stack::Stack>(tab)
                .is_some()
        );
        assert_eq!(
            app.world().get::<PageMetadata>(tab).unwrap().url,
            TERMINAL_WEBVIEW_URL
        );
    }

    #[test]
    fn terminal_send_writes_raw_text_to_active_terminal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(crate::layout::stack::stack_bundle())
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

        let pending = app
            .world()
            .get::<PendingTerminalInput>(terminal)
            .expect("PendingTerminalInput inserted");
        assert_eq!(pending.data, b"ls".to_vec());
    }

    #[test]
    fn browser_navigate_auto_spawns_tab_when_pane_is_empty() {
        use crate::browser::Browser;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedStack>().stack = None;

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: None,
                },
            });

        app.update();

        let world = app.world_mut();
        let mut tabs = world.query_filtered::<&ChildOf, With<crate::layout::stack::Stack>>();
        let tab_count_under_pane = tabs
            .iter(world)
            .filter(|child_of| child_of.get() == pane)
            .count();
        assert_eq!(
            tab_count_under_pane, 1,
            "browser_navigate should have spawned exactly one tab in the focused pane"
        );

        let mut tab_metadata =
            world.query_filtered::<&PageMetadata, With<crate::layout::stack::Stack>>();
        let tab_urls: Vec<String> = tab_metadata.iter(world).map(|p| p.url.clone()).collect();
        assert!(
            tab_urls.contains(&"https://example.com".to_string()),
            "tab entity should have PageMetadata with the URL; found {tab_urls:?}"
        );

        let mut browsers = world.query::<(&Browser, &PageMetadata)>();
        let urls: Vec<String> = browsers.iter(world).map(|(_, p)| p.url.clone()).collect();
        assert!(
            urls.contains(&"https://example.com".to_string()),
            "browser entity with the URL should exist; found {urls:?}"
        );
    }

    #[test]
    fn browser_navigate_targets_specific_pane_when_id_provided() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane_a = app.world_mut().spawn(Pane).id();
        let pane_b = app.world_mut().spawn(Pane).id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane_a);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: Some(pane_b.to_bits().to_string()),
                },
            });

        app.update();

        let world = app.world_mut();
        let mut tabs = world.query_filtered::<&ChildOf, With<crate::layout::stack::Stack>>();
        let tabs_in_b = tabs
            .iter(world)
            .filter(|child_of| child_of.get() == pane_b)
            .count();
        let tabs_in_a = tabs
            .iter(world)
            .filter(|child_of| child_of.get() == pane_a)
            .count();
        assert_eq!(tabs_in_b, 1, "tab should be spawned in target pane B");
        assert_eq!(tabs_in_a, 0, "no tab should be spawned in focused pane A");
    }

    #[test]
    fn split_and_navigate_with_terminal_url_spawns_terminal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::SplitAndNavigate {
                    direction: "right".to_string(),
                    url: "vmux://terminal/".to_string(),
                },
            });

        app.update();

        let world = app.world_mut();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert!(terminal_count >= 1, "expected at least one Terminal entity");
    }

    #[test]
    fn split_and_navigate_with_terminal_url_and_cwd_query_uses_cwd() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        let cwd = std::env::current_dir().unwrap().display().to_string();
        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::SplitAndNavigate {
                    direction: "right".to_string(),
                    url: format!("vmux://terminal/?cwd={cwd}"),
                },
            });

        app.update();

        let world = app.world_mut();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert!(
            terminal_count >= 1,
            "expected at least one Terminal entity (cwd path was valid)"
        );
    }

    #[test]
    fn split_and_navigate_with_terminal_url_and_invalid_cwd_errors() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::SplitAndNavigate {
                    direction: "right".to_string(),
                    url: "vmux://terminal/?cwd=/this/does/not/exist".to_string(),
                },
            });

        app.update();

        let world = app.world_mut();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert_eq!(
            terminal_count, 0,
            "no terminal should be spawned with invalid cwd"
        );
    }

    #[test]
    fn split_and_navigate_with_sessions_url_spawns_sessions_view() {
        use crate::spaces::SpacesView;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::SplitAndNavigate {
                    direction: "right".to_string(),
                    url: "vmux://sessions/".to_string(),
                },
            });

        app.update();

        let world = app.world_mut();
        let count = world.query::<&SpacesView>().iter(world).count();
        assert!(count >= 1, "expected at least one SpacesView entity");
    }

    #[test]
    fn split_and_navigate_with_processes_url_spawns_processes_monitor() {
        use crate::processes_monitor::ProcessesMonitor;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::SplitAndNavigate {
                    direction: "right".to_string(),
                    url: "vmux://services/".to_string(),
                },
            });

        app.update();

        let world = app.world_mut();
        let count = world.query::<&ProcessesMonitor>().iter(world).count();
        assert!(count >= 1, "expected at least one ProcessesMonitor entity");
    }

    #[test]
    fn split_and_navigate_with_unknown_vmux_url_errors() {
        use crate::browser::Browser;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::SplitAndNavigate {
                    direction: "right".to_string(),
                    url: "vmux://nonsense/".to_string(),
                },
            });

        app.update();

        let world = app.world_mut();
        let browser_count = world.query::<&Browser>().iter(world).count();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert_eq!(
            browser_count, 0,
            "no browser should be spawned for unknown vmux URL"
        );
        assert_eq!(
            terminal_count, 0,
            "no terminal should be spawned for unknown vmux URL"
        );
    }

    #[test]
    fn browser_navigate_with_terminal_url_spawns_terminal_in_focused_pane() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://terminal/".to_string(),
                    pane: None,
                },
            });

        app.update();

        let world = app.world_mut();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert!(
            terminal_count >= 1,
            "terminal should be spawned in focused pane"
        );
    }

    #[test]
    fn browser_navigate_with_terminal_url_and_target_pane_uses_target() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane_a = app.world_mut().spawn(Pane).id();
        let pane_b = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane_a);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://terminal/".to_string(),
                    pane: Some(pane_b.to_bits().to_string()),
                },
            });

        app.update();

        let world = app.world_mut();
        let mut terminals = world.query_filtered::<&ChildOf, With<Terminal>>();
        let term_parents: Vec<Entity> = terminals.iter(world).map(|c| c.get()).collect();
        let mut found_in_b = 0;
        let mut found_in_a = 0;
        for tab in &term_parents {
            if let Some(co) = world.get::<ChildOf>(*tab) {
                if co.get() == pane_b {
                    found_in_b += 1;
                } else if co.get() == pane_a {
                    found_in_a += 1;
                }
            }
        }
        assert_eq!(found_in_b, 1, "terminal should be in target pane B");
        assert_eq!(found_in_a, 0, "no terminal in focused pane A");
    }

    #[test]
    fn browser_navigate_with_unknown_vmux_url_errors() {
        use crate::browser::Browser;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://nonsense/".to_string(),
                    pane: None,
                },
            });

        app.update();

        let world = app.world_mut();
        let browser_count = world.query::<&Browser>().iter(world).count();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert_eq!(
            browser_count, 0,
            "no browser should be spawned for unknown vmux URL"
        );
        assert_eq!(
            terminal_count, 0,
            "no terminal should be spawned for unknown vmux URL"
        );
    }

    #[test]
    fn split_and_navigate_creates_split_and_browser_tab() {
        use crate::browser::Browser;
        use vmux_layout::pane::PaneSplit;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::SplitAndNavigate {
                    direction: "right".to_string(),
                    url: "https://example.com".to_string(),
                },
            });

        app.update();

        let world = app.world_mut();
        assert!(
            world.get::<PaneSplit>(pane).is_some(),
            "active pane should now be a PaneSplit"
        );

        let mut browsers = world.query::<(&Browser, &PageMetadata)>();
        let urls: Vec<String> = browsers.iter(world).map(|(_, p)| p.url.clone()).collect();
        assert!(
            urls.contains(&"https://example.com".to_string()),
            "browser entity with the URL should exist; found {urls:?}"
        );
    }

    #[test]
    fn browser_navigate_with_claude_url_does_not_spawn_standalone_browser() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://claude/".into(),
                    pane: None,
                },
            });

        app.update();

        let world = app.world_mut();
        let standalone_browser_count = world
            .query_filtered::<&Browser, Without<Terminal>>()
            .iter(world)
            .count();
        assert_eq!(
            standalone_browser_count, 0,
            "claude URL should never spawn a standalone browser tab"
        );
    }

    #[test]
    fn browser_navigate_with_codex_url_does_not_spawn_standalone_browser() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://codex/".into(),
                    pane: None,
                },
            });

        app.update();

        let world = app.world_mut();
        let standalone_browser_count = world
            .query_filtered::<&Browser, Without<Terminal>>()
            .iter(world)
            .count();
        assert_eq!(
            standalone_browser_count, 0,
            "codex URL should never spawn a standalone browser tab"
        );
    }

    #[test]
    fn deep_link_focuses_existing_claude_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<vmux_agent::session::AgentSessionToEntity>();
        app.add_systems(Update, vmux_agent::session::track_session_id_inserts);

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
            .resource::<vmux_agent::session::AgentSessionToEntity>();
        assert_eq!(
            map.0.get(&(AgentKind::Claude, "dl-1".into())),
            Some(&entity)
        );
    }

    #[test]
    fn agent_plugin_registers_all_six_provider_entries() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        let providers = app.world().resource::<AgentProviders>();
        for id in [
            "vibe_new",
            "vibe_new_stack",
            "claude_new",
            "claude_new_stack",
            "codex_new",
            "codex_new_stack",
        ] {
            assert!(providers.contains(id), "missing provider: {id}");
        }
    }

    #[test]
    fn update_settings_via_apply_mutates_resource_and_returns_ron() {
        let mut settings = test_settings();
        assert!(!settings.auto_update);
        let ron_bytes = crate::settings::apply_settings_update(
            &mut settings,
            "auto_update",
            serde_json::json!(true),
        )
        .expect("apply ok");
        assert!(settings.auto_update);
        assert!(ron_bytes.contains("auto_update"));
    }
}
