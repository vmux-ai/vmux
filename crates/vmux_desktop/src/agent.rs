use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    command::{AppCommand, WriteAppCommands},
    layout::{
        pane::{Pane, PaneSplit},
        tab::FocusedTab,
    },
    settings::AppSettings,
    terminal::{PendingTerminalInput, ProcessExited, ServiceMessageSet, Terminal},
};
use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::{CefKeyboardTarget, RequestNavigate, WebviewExtendStandardMaterial};

use crate::browser::Browser;
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
    pub(crate) cwd: PathBuf,
    pub(crate) command: String,
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
    pub(crate) fn register(&mut self, provider: AgentProvider) {
        self.providers.insert(provider.id, provider);
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    #[cfg(test)]
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

#[derive(Message)]
pub(crate) struct AgentLaunchRequested {
    pub(crate) provider_id: String,
    pub(crate) cwd: PathBuf,
}

pub(crate) struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AgentProviders>()
            .add_message::<AgentCommandRequest>()
            .add_message::<AgentQueryRequest>()
            .add_message::<AgentLaunchRequested>()
            .add_systems(
                Update,
                (
                    handle_agent_launch_requests,
                    handle_agent_commands,
                    crate::agent_query::handle_agent_queries,
                )
                    .chain()
                    .in_set(WriteAppCommands)
                    .after(ServiceMessageSet),
            );
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
            crate::layout::tab::tab_bundle(),
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

pub(crate) fn spawn_browser_tab(
    pane: Entity,
    url: &str,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    let tab = commands
        .spawn((
            crate::layout::tab::tab_bundle(),
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

fn handle_agent_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    mut app_commands: MessageWriter<AppCommand>,
    focus: Res<FocusedTab>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    settings: Res<AppSettings>,
    service: Option<Res<crate::terminal::ServiceClient>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    use vmux_service::protocol::{AgentCommandResult, ClientMessage};

    for request in reader.read() {
        let result = match &request.command {
            ServiceAgentCommand::AppCommand { id } => {
                if let Some(command) = AppCommand::from_agent_id(id) {
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
                                &mut meshes,
                                &mut webview_mt,
                                &settings,
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
                    && let Some(terminal) = active_terminal_for_tab(focus.tab, &terminals)
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
                                &mut meshes,
                                &mut webview_mt,
                                &settings,
                            );
                            AgentCommandResult::Ok
                        }
                        Err(message) => AgentCommandResult::Error(message),
                    }
                } else {
                    AgentCommandResult::Error("no active pane".to_string())
                }
            }
            ServiceAgentCommand::BrowserNavigate { url } => {
                if let Some(webview) = active_webview_for_tab(focus.tab, &browsers, &terminals) {
                    commands.trigger(RequestNavigate {
                        webview,
                        url: url.clone(),
                    });
                    AgentCommandResult::Ok
                } else if let Some(pane) = focus.pane.filter(|pane| panes.contains(*pane)) {
                    spawn_browser_tab(pane, url, &mut commands, &mut meshes, &mut webview_mt);
                    AgentCommandResult::Ok
                } else {
                    AgentCommandResult::Error("browser_navigate: no focused pane".to_string())
                }
            }
            ServiceAgentCommand::TerminalSend { text } => {
                if let Some(terminal) = active_terminal_for_tab(focus.tab, &terminals) {
                    commands.entity(terminal).insert(PendingTerminalInput {
                        data: text.as_bytes().to_vec(),
                    });
                    AgentCommandResult::Ok
                } else {
                    AgentCommandResult::Error("terminal_send: no active terminal".to_string())
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

fn handle_agent_launch_requests(
    mut reader: MessageReader<AgentLaunchRequested>,
    providers: Res<AgentProviders>,
    focus: Res<FocusedTab>,
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
        spawn_terminal_tab(
            pane,
            Some(&prepared.cwd),
            Some(shell_command_input(&prepared.command)),
            &mut commands,
            &mut meshes,
            &mut webview_mt,
            &settings,
        );
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
            cwd: cwd.to_path_buf(),
            command: "echo agent".to_string(),
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
        app.insert_resource(FocusedTab::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.init_resource::<CapturedNavigateUrls>();

        let pane = app.world_mut().spawn(Pane).id();
        let tab = app
            .world_mut()
            .spawn(crate::layout::tab::tab_bundle())
            .insert(ChildOf(pane))
            .id();
        app.world_mut().spawn(Browser).insert(ChildOf(tab));

        app.world_mut().resource_mut::<FocusedTab>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedTab>().tab = Some(tab);

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
        app.insert_resource(FocusedTab::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedTab>().pane = Some(pane);
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

        let mut terminals = app
            .world_mut()
            .query::<(&Terminal, &PendingTerminalInput, &ChildOf)>();
        let rows = terminals
            .iter(app.world())
            .map(|(_, input, child_of)| (input.data.clone(), child_of.get()))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, b"echo agent\r".to_vec());

        let tab = rows[0].1;
        assert!(app.world().get::<crate::layout::tab::Tab>(tab).is_some());
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
        app.insert_resource(FocusedTab::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        let tab = app
            .world_mut()
            .spawn(crate::layout::tab::tab_bundle())
            .insert(ChildOf(pane))
            .id();
        let terminal = app.world_mut().spawn(Terminal).insert(ChildOf(tab)).id();

        app.world_mut().resource_mut::<FocusedTab>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedTab>().tab = Some(tab);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::TerminalSend {
                    text: "ls".to_string(),
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
        app.insert_resource(FocusedTab::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();

        app.world_mut().resource_mut::<FocusedTab>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedTab>().tab = None;

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "https://example.com".to_string(),
                },
            });

        app.update();

        let world = app.world_mut();
        let mut tabs = world.query_filtered::<&ChildOf, With<crate::layout::tab::Tab>>();
        let tab_count_under_pane = tabs
            .iter(world)
            .filter(|child_of| child_of.get() == pane)
            .count();
        assert_eq!(
            tab_count_under_pane, 1,
            "browser_navigate should have spawned exactly one tab in the focused pane"
        );

        let mut tab_metadata =
            world.query_filtered::<&PageMetadata, With<crate::layout::tab::Tab>>();
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
}
