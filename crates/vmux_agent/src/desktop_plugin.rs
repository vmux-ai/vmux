use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use bevy::prelude::*;
use bevy_cef::prelude::{CefKeyboardTarget, WebviewExtendStandardMaterial};
use vmux_command::{AppCommand, WriteAppCommands};
use vmux_core::LastActivatedAt;
use vmux_core::PageMetadata;
use vmux_core::agent::AgentKind;
use vmux_layout::event::TERMINAL_WEBVIEW_URL;
use vmux_layout::{
    pane::{Pane, PaneSplit},
    stack::FocusedStack,
};
use vmux_service::protocol::{AgentCommand as ServiceAgentCommand, AgentShellMode};
use vmux_settings::AppSettings;
use vmux_space::{ActiveSpace, SpacesView};
use vmux_terminal::ProcessExited;
use vmux_terminal::{ServiceMessageSet, new_terminal_bundle_with_cwd};

use crate::AgentVariant;
use crate::events::{AgentCommandRequest, AgentQueryRequest};
use crate::session::{AgentSession, PendingAgentSession, SessionId};
use crate::strategy::AgentStrategies;

pub use vmux_space::cwd::{default_space_dir, space_dir, valid_cwd};

#[derive(Clone)]
pub struct AgentProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub shortcut: &'static str,
    pub executable: &'static str,
    pub available: fn() -> bool,
    pub prepare: fn(&Path) -> Result<PreparedAgentLaunch, String>,
}

pub struct PreparedAgentLaunch {
    pub kind: AgentKind,
    pub cwd: PathBuf,
    pub launch: vmux_terminal::launch::TerminalLaunch,
}

pub struct AgentCommandEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub shortcut: &'static str,
}

#[derive(Resource, Default)]
pub struct AgentProviders {
    providers: BTreeMap<&'static str, AgentProvider>,
}

impl AgentProviders {
    #[allow(dead_code)]
    pub fn register(&mut self, provider: AgentProvider) {
        self.providers.insert(provider.id, provider);
    }

    pub fn contains(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get(&self, id: &str) -> Option<&AgentProvider> {
        self.providers.get(id)
    }

    pub fn command_entries(&self) -> Vec<AgentCommandEntry> {
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

#[derive(Message)]
pub struct AgentLaunchRequested {
    pub provider_id: String,
    pub cwd: PathBuf,
}

fn vibe_available() -> bool {
    crate::exec::find_executable("vibe").is_some()
}

fn claude_available() -> bool {
    crate::exec::find_executable("claude").is_some()
}

fn codex_available() -> bool {
    crate::exec::find_executable("codex").is_some()
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
    use crate::claude::ClaudeStrategy;
    use crate::codex::CodexStrategy;
    use crate::vibe::VibeStrategy;
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(VibeStrategy));
    strategies.register_cli(Box::new(ClaudeStrategy));
    strategies.register_cli(Box::new(CodexStrategy));
    let launch = crate::build_agent_launch(kind, cwd, None, &strategies)?;
    Ok(PreparedAgentLaunch {
        kind,
        cwd: cwd.to_path_buf(),
        launch,
    })
}

pub struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AgentProviders>()
            .add_message::<AgentCommandRequest>()
            .add_message::<AgentQueryRequest>()
            .add_message::<AgentLaunchRequested>()
            .add_message::<crate::AgentSessionExited>()
            .add_message::<vmux_settings::SettingsWriteRequest>()
            .add_message::<vmux_layout::BrowserNavigateRequest>()
            .add_message::<vmux_terminal::TerminalSendRequest>()
            .add_message::<vmux_terminal::RunShellRequest>()
            .add_message::<vmux_layout::reconcile::LayoutApplyRequest>()
            .add_message::<vmux_layout::reconcile::LayoutSnapshotRequest>()
            .add_systems(
                Update,
                (
                    handle_agent_launch_requests,
                    handle_agent_commands,
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

pub use crate::build_agent_launch;
pub use vmux_terminal::spawn_terminal_tab;

pub fn spawn_fresh_agent_tab(
    kind: AgentKind,
    pane: Entity,
    cwd: PathBuf,
    strategies: &AgentStrategies,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Result<Entity, String> {
    let launch = crate::build_agent_launch(kind, &cwd, None, strategies)?;
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

pub fn spawn_agent_resume_tab(
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
    let launch = crate::build_agent_launch(kind, &cwd, Some(&session_id), strategies)?;
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

pub fn spawn_process_tab(
    pane: Entity,
    command: String,
    args: Vec<String>,
    cwd: PathBuf,
    env: Vec<(String, String)>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Entity {
    let tab = commands
        .spawn((
            vmux_layout::stack::stack_bundle(),
            LastActivatedAt::now(),
            ChildOf(pane),
        ))
        .id();
    let title = std::path::Path::new(&command)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&command)
        .to_string();
    commands.entity(tab).insert(PageMetadata {
        url: TERMINAL_WEBVIEW_URL.to_string(),
        title,
        bg_color: Some(vmux_layout::event::TERMINAL_CHROME_BG_COLOR.to_string()),
        ..default()
    });
    let launch = vmux_terminal::launch::TerminalLaunch {
        command,
        args,
        cwd: cwd.to_string_lossy().to_string(),
        env,
        kind: vmux_terminal::launch::TerminalKind::Plain,
    };
    let term = commands
        .spawn((
            new_terminal_bundle_with_cwd(meshes, webview_mt, settings, Some(&cwd)),
            ChildOf(tab),
        ))
        .id();
    commands.entity(term).insert((launch, CefKeyboardTarget));
    tab
}

pub fn spawn_browser_tab(
    pane: Entity,
    url: &str,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    let tab = commands
        .spawn((
            vmux_layout::stack::stack_bundle(),
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
        vmux_layout::Browser::new(meshes, webview_mt, url),
        ChildOf(tab),
    ));
    tab
}

pub fn spawn_app_agent_tab(
    provider: &str,
    model: &str,
    pane: Entity,
    sid: &str,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    strategies: &AgentStrategies,
) -> Option<Entity> {
    let tab = commands
        .spawn((
            vmux_layout::stack::stack_bundle(),
            LastActivatedAt::now(),
            ChildOf(pane),
        ))
        .id();
    attach_app_agent_to_stack(
        tab, provider, model, sid, commands, meshes, webview_mt, strategies,
    )?;
    Some(tab)
}

pub fn attach_app_agent_to_stack(
    stack: Entity,
    provider: &str,
    model: &str,
    sid: &str,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    strategies: &AgentStrategies,
) -> Option<()> {
    let strategy = strategies.get_app_by_provider_model(provider, model)?;
    let kind = strategy.kind();
    let url = format!("{}{sid}", crate::kind::app_url_prefix(provider, model));
    commands.entity(stack).insert(PageMetadata {
        url: url.clone(),
        title: format!("{provider}/{model}"),
        bg_color: Some(vmux_layout::event::TERMINAL_CHROME_BG_COLOR.to_string()),
        ..default()
    });
    commands.entity(stack).insert((
        crate::components::AgentSession {
            kind,
            variant: AgentVariant::App,
            sid: sid.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
        },
        crate::AgentMessages::default(),
        crate::AgentApprovalPolicy::default(),
        crate::AgentRunState::default(),
    ));
    let placeholder = app_agent_placeholder_url(provider, model, sid);
    commands.spawn((
        vmux_layout::Browser::new(meshes, webview_mt, &placeholder),
        ChildOf(stack),
    ));
    Some(())
}

pub fn app_agent_placeholder_url(provider: &str, model: &str, sid: &str) -> String {
    let html = format!(
        "<!doctype html><html><head><meta charset='utf-8'><title>App Agent</title><style>html,body{{height:100%;margin:0;background:#0c0c10;color:#bbb;font-family:-apple-system,BlinkMacSystemFont,sans-serif;display:flex;align-items:center;justify-content:center}}div{{text-align:center;padding:2rem}}h1{{margin:0 0 0.5rem;font-weight:600;color:#eee}}code{{background:#1a1a22;padding:0.15rem 0.4rem;border-radius:4px;color:#e0a050}}</style></head><body><div><h1>App Agent</h1><p><code>{provider}</code> / <code>{model}</code></p><p>Session <code>{sid}</code></p><p style='opacity:0.6;margin-top:1rem'>Native chat UI ships in step 4 of the App agent design.</p></div></body></html>"
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

pub fn parse_app_agent_url(url: &str) -> Option<(String, String, Option<String>)> {
    let body = url.strip_prefix("vmux://agent/")?;
    let segs: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        [provider, model] => Some(((*provider).to_string(), (*model).to_string(), None)),
        [provider, model, sid] => Some((
            (*provider).to_string(),
            (*model).to_string(),
            Some((*sid).to_string()),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod app_agent_url_tests {
    use super::*;

    #[test]
    fn parse_app_agent_url_provider_model_only() {
        let (provider, model, sid) = parse_app_agent_url("vmux://agent/openai/gpt-5.5").unwrap();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-5.5");
        assert!(sid.is_none());
    }

    #[test]
    fn parse_app_agent_url_with_sid() {
        let (provider, model, sid) =
            parse_app_agent_url("vmux://agent/anthropic/claude-opus-4.7/xHigh").unwrap();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-opus-4.7");
        assert_eq!(sid.as_deref(), Some("xHigh"));
    }

    #[test]
    fn parse_app_agent_url_rejects_single_segment() {
        assert!(parse_app_agent_url("vmux://agent/vibe").is_none());
    }

    #[test]
    fn parse_app_agent_url_rejects_too_many_segments() {
        assert!(parse_app_agent_url("vmux://agent/openai/gpt/sid/extra").is_none());
    }

    #[test]
    fn parse_app_agent_url_rejects_non_agent_host() {
        assert!(parse_app_agent_url("https://google.com").is_none());
    }
}

pub fn spawn_sessions_tab(
    pane: Entity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    let tab = commands
        .spawn((
            vmux_layout::stack::stack_bundle(),
            LastActivatedAt::now(),
            ChildOf(pane),
        ))
        .id();
    commands.entity(tab).insert(PageMetadata {
        url: vmux_space::event::SPACES_WEBVIEW_URL.to_string(),
        title: "Sessions".to_string(),
        ..default()
    });
    commands.spawn((SpacesView::new(meshes, webview_mt), ChildOf(tab)));
    tab
}

pub fn spawn_processes_tab(
    pane: Entity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    let tab = commands
        .spawn((
            vmux_layout::stack::stack_bundle(),
            LastActivatedAt::now(),
            ChildOf(pane),
        ))
        .id();
    commands.entity(tab).insert(PageMetadata {
        url: vmux_layout::event::SERVICES_WEBVIEW_URL.to_string(),
        title: "Background Services".to_string(),
        bg_color: Some(vmux_layout::event::TERMINAL_CHROME_BG_COLOR.to_string()),
        ..default()
    });
    commands.spawn((
        vmux_terminal::processes_monitor::ProcessesMonitor::new(meshes, webview_mt),
        ChildOf(tab),
    ));
    tab
}

pub fn spawn_vmux_tab(
    url: &str,
    pane: Entity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
    pid_to_entity: Option<&vmux_terminal::pid::PidToEntity>,
    agent_to_entity: Option<&crate::session::AgentSessionToEntity>,
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
                            vmux_terminal::pid::focus_pane_entity(entity, commands, child_of_q);
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
        "agent" => {
            let path = parsed.path().trim_start_matches('/');
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            let agent_url = crate::AgentUrl::parse(url);
            match agent_url {
                Some(crate::AgentUrl::App {
                    provider,
                    model,
                    sid,
                }) => {
                    if spawn_app_agent_tab(
                        &provider, &model, pane, &sid, commands, meshes, webview_mt, strategies,
                    )
                    .is_none()
                    {
                        return Err(format!(
                            "no App agent strategy registered for {provider}/{model}"
                        ));
                    }
                    Ok(())
                }
                Some(crate::AgentUrl::Cli { kind, sid }) => {
                    let cwd =
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
                    if let Some(map) = agent_to_entity
                        && let Some(&entity) = map.0.get(&(kind, sid.clone()))
                    {
                        vmux_terminal::pid::focus_pane_entity(entity, commands, child_of_q);
                        return Ok(());
                    }
                    if let Err(e) = spawn_agent_resume_tab(
                        kind, pane, cwd, sid, strategies, commands, meshes, webview_mt, settings,
                    ) {
                        bevy::log::warn!(
                            "spawn_agent_resume_tab({kind:?}) failed: {e}; falling back to terminal"
                        );
                        spawn_terminal_tab(
                            pane, None, None, commands, meshes, webview_mt, settings,
                        );
                    }
                    Ok(())
                }
                None => {
                    if segs.len() == 1
                        && let Some(kind) = AgentKind::from_url_segment(segs[0])
                    {
                        let cwd = std::env::current_dir()
                            .unwrap_or_else(|_| std::path::PathBuf::from("/"));
                        if let Err(e) = spawn_fresh_agent_tab(
                            kind, pane, cwd, strategies, commands, meshes, webview_mt, settings,
                        ) {
                            bevy::log::warn!(
                                "spawn_fresh_agent_tab({kind:?}) failed: {e}; falling back to terminal"
                            );
                            spawn_terminal_tab(
                                pane, None, None, commands, meshes, webview_mt, settings,
                            );
                        }
                        return Ok(());
                    }
                    if segs.len() == 2 {
                        let provider = segs[0];
                        let model = segs[1];
                        let sid = uuid::Uuid::new_v4().to_string();
                        if spawn_app_agent_tab(
                            provider, model, pane, &sid, commands, meshes, webview_mt, strategies,
                        )
                        .is_none()
                        {
                            return Err(format!(
                                "no App agent strategy registered for {provider}/{model}"
                            ));
                        }
                        return Ok(());
                    }
                    Err(format!("malformed agent URL '{url}'"))
                }
            }
        }
        other => Err(format!("unknown vmux URL host '{other}' in '{url}'")),
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct SpawnAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    webview_mt: ResMut<'w, Assets<WebviewExtendStandardMaterial>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct SettingsParams<'w> {
    settings: ResMut<'w, AppSettings>,
    writes: MessageWriter<'w, vmux_settings::SettingsWriteRequest>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct AgentLookups<'w> {
    pub pid_to_entity: Option<Res<'w, vmux_terminal::pid::PidToEntity>>,
    pub agent_to_entity: Option<Res<'w, crate::session::AgentSessionToEntity>>,
    pub active_space: Option<Res<'w, ActiveSpace>>,
}

fn handle_agent_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    mut app_commands: MessageWriter<AppCommand>,
    mut browser_nav_writer: MessageWriter<vmux_layout::BrowserNavigateRequest>,
    mut terminal_send_writer: MessageWriter<vmux_terminal::TerminalSendRequest>,
    mut run_shell_writer: MessageWriter<vmux_terminal::RunShellRequest>,
    focus: Res<FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    lookups: AgentLookups,
    mut sp: SettingsParams,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    mut layout_apply_writer: MessageWriter<vmux_layout::reconcile::LayoutApplyRequest>,
    mut commands: Commands,
    mut assets: SpawnAssets,
) {
    let active_space = lookups.active_space.as_deref();
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
                                .map(|s| space_dir(&s.record.id))
                                .unwrap_or_else(default_space_dir)
                        });
                        if command.trim().is_empty() {
                            spawn_terminal_tab(
                                pane,
                                Some(&cwd_path),
                                None,
                                &mut commands,
                                &mut assets.meshes,
                                &mut assets.webview_mt,
                                &sp.settings,
                            );
                        } else {
                            spawn_process_tab(
                                pane,
                                command.clone(),
                                args.clone(),
                                cwd_path,
                                env.clone(),
                                &mut commands,
                                &mut assets.meshes,
                                &mut assets.webview_mt,
                                &sp.settings,
                            );
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
            ServiceAgentCommand::UpdateSettings { path, value_json } => {
                match serde_json::from_str::<serde_json::Value>(value_json) {
                    Ok(value) => match vmux_settings::apply_settings_update(
                        sp.settings.as_mut(),
                        path,
                        value,
                    ) {
                        Ok(ron_bytes) => {
                            sp.writes
                                .write(vmux_settings::SettingsWriteRequest { ron_bytes });
                            AgentCommandResult::Ok
                        }
                        Err(message) => AgentCommandResult::Error(message),
                    },
                    Err(e) => AgentCommandResult::Error(format!(
                        "update_settings: invalid JSON value: {e}"
                    )),
                }
            }
            ServiceAgentCommand::UpdateLayout { layout } => {
                layout_apply_writer.write(vmux_layout::reconcile::LayoutApplyRequest {
                    request_id: request.request_id.0,
                    snapshot: layout.clone(),
                });
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

#[allow(clippy::type_complexity)]
pub fn detect_agent_session_process_exit(
    mut commands: Commands,
    mut writer: MessageWriter<crate::AgentSessionExited>,
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
                format!("{}{p}", vmux_terminal::event::TERMINAL_WEBVIEW_URL)
            }
            None => vmux_terminal::event::TERMINAL_WEBVIEW_URL.to_string(),
        };
        if meta.url != next {
            meta.url = next;
        }
        writer.write(crate::AgentSessionExited { entity });
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
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_settings::{BrowserSettings, ShortcutSettings};
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
            startup_url: None,
            agent: vmux_settings::AgentSettings::default(),
        }
    }

    #[test]
    fn blank_cwd_is_accepted() {
        assert_eq!(valid_cwd("").unwrap(), None);
    }

    fn fake_prepare(cwd: &std::path::Path) -> Result<PreparedAgentLaunch, String> {
        Ok(PreparedAgentLaunch {
            kind: AgentKind::Vibe,
            cwd: cwd.to_path_buf(),
            launch: vmux_terminal::launch::TerminalLaunch {
                command: "echo".to_string(),
                args: vec!["agent".to_string()],
                cwd: cwd.to_string_lossy().to_string(),
                env: vec![],
                kind: vmux_terminal::launch::TerminalKind::Vibe,
            },
        })
    }

    #[test]
    fn agent_launch_request_uses_registered_provider_to_spawn_terminal_tab() {
        use bevy::ecs::relationship::Relationship;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
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

        let mut terminals =
            app.world_mut()
                .query::<(&Terminal, &vmux_terminal::launch::TerminalLaunch, &ChildOf)>();
        let rows: Vec<_> = terminals
            .iter(app.world())
            .map(|(_, launch, child_of)| (launch.command.clone(), child_of.get()))
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "echo");

        let tab = rows[0].1;
        assert!(app.world().get::<vmux_layout::stack::Stack>(tab).is_some());
        assert_eq!(
            app.world().get::<PageMetadata>(tab).unwrap().url,
            TERMINAL_WEBVIEW_URL
        );
    }

    #[test]
    fn deep_link_focuses_existing_claude_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<crate::session::AgentSessionToEntity>();
        app.add_systems(Update, crate::session::track_session_id_inserts);

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
    fn agent_plugin_registers_all_six_provider_entries() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
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
        let ron_bytes = vmux_settings::apply_settings_update(
            &mut settings,
            "auto_update",
            serde_json::json!(true),
        )
        .expect("apply ok");
        assert!(settings.auto_update);
        assert!(ron_bytes.contains("auto_update"));
    }
}
