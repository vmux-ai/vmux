use std::path::PathBuf;

use bevy::prelude::*;
use vmux_command::{CommandDefinition, CommandInvocation};
use vmux_layout::{
    pane::{Pane, PaneSplit},
    stack::FocusedStack,
};
use vmux_service::client::ServiceRequest;
use vmux_service::protocol::{
    AgentBookmarkCommand, AgentCommand as ServiceAgentCommand, AgentCommandResult, AgentRequestId,
    AgentShellMode, AgentSpaceCommand, SharedAgentCommand,
};
use vmux_setting::AppSettings;
use vmux_space::ActiveSpace;
use vmux_terminal::TerminalStackSpawnRequest;

use crate::events::{AgentCommandRequest, CommandOrigin};

use crate::host::browser_pane::AgentBrowserResolve;
use crate::host::valid_cwd;

use super::CommandSet;

pub(super) struct DispatchPlugin;

impl Plugin for DispatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_systems(
                Update,
                (
                    forward_history_open_intent.in_set(CommandSet::History),
                    (
                        handle_command_invocations,
                        handle_terminal_commands,
                        handle_browser_commands,
                        handle_desktop_commands,
                        handle_space_commands,
                        handle_bookmark_commands,
                        handle_shared_commands,
                    )
                        .in_set(CommandSet::Commands),
                ),
            )
            .add_systems(
                Update,
                (handle_focus_pane_requests, handle_rename_profile_requests)
                    .after(CommandSet::Commands),
            );
    }
}

#[derive(Message, Clone)]
pub(crate) struct ProcessStackSpawnRequest {
    pub(crate) pane: Entity,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) activate: bool,
}

#[derive(Message, Clone)]
pub(crate) struct FocusPaneRequest {
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
pub(crate) struct RenameProfileRequest {
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

fn remote_agents(
    snapshot: &vmux_command::snapshot::CommandBarAgentsSnapshot,
) -> Vec<vmux_api::room::RemoteAgent> {
    snapshot
        .acp
        .iter()
        .map(|agent| vmux_api::room::RemoteAgent {
            id: agent.id.clone(),
            name: agent.name.clone(),
            url: agent.url.clone(),
            icon: agent.icon.clone(),
        })
        .chain(
            snapshot
                .providers
                .iter()
                .map(|agent| vmux_api::room::RemoteAgent {
                    id: agent.id.clone(),
                    name: format!("{} (CLI)", agent.name),
                    url: agent.url.clone(),
                    icon: agent.icon.clone(),
                }),
        )
        .collect()
}

fn handle_command_invocations(
    mut reader: MessageReader<AgentCommandRequest>,
    command_definitions: Query<&CommandDefinition>,
    mut command_invocations: MessageWriter<CommandInvocation>,
    agents: Query<(
        Entity,
        &vmux_core::team::Agent,
        Option<&vmux_service::protocol::ProcessId>,
    )>,
    user: Query<Entity, With<vmux_core::team::User>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let result = match &request.command {
            ServiceAgentCommand::FileTouched { .. }
            | ServiceAgentCommand::FileSearch { .. }
            | ServiceAgentCommand::TurnEnded { .. } => AgentCommandResult::Ok,
            ServiceAgentCommand::InvokeCommand { id, args } => {
                let args = match vmux_core::JsonArguments::try_from(args) {
                    Ok(args) => args.0,
                    Err(message) => {
                        service_requests.write(ServiceRequest(
                            request.response(AgentCommandResult::Error(message)),
                        ));
                        continue;
                    }
                };
                let caller = match &request.origin {
                    CommandOrigin::Agent {
                        anchor: Some(pid), ..
                    } => agents
                        .iter()
                        .find(|(_, _, process)| process.is_some_and(|process| process == pid))
                        .map(|(entity, _, _)| entity),
                    CommandOrigin::Agent { sid: Some(sid), .. } if !sid.is_empty() => agents
                        .iter()
                        .find(|(_, agent, _)| &agent.sid == sid)
                        .map(|(entity, _, _)| entity),
                    CommandOrigin::User => user.single().ok(),
                    _ => None,
                }
                .unwrap_or(Entity::PLACEHOLDER);
                let Some(definition) = command_definitions
                    .iter()
                    .find(|definition| definition.matches(id))
                else {
                    service_requests.write(ServiceRequest(request.response(
                        AgentCommandResult::Error(format!("unknown app command: {id}")),
                    )));
                    continue;
                };
                let invocation = if request.origin.is_agent() {
                    definition.agent_invocation(caller, args)
                } else {
                    definition.user_invocation(caller, args)
                };
                match invocation {
                    Ok(invocation) => {
                        command_invocations.write(invocation);
                        AgentCommandResult::Ok
                    }
                    Err(message) => AgentCommandResult::Error(message),
                }
            }
            _ => continue,
        };
        service_requests.write(ServiceRequest(request.response(result)));
    }
}

fn handle_terminal_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    focus: Res<FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_space: Option<Res<ActiveSpace>>,
    settings: Res<AppSettings>,
    mut terminal_send: MessageWriter<vmux_terminal::TerminalSendRequest>,
    mut run_shell: MessageWriter<vmux_terminal::RunShellRequest>,
    mut terminal_spawn: MessageWriter<TerminalStackSpawnRequest>,
    mut process_spawn: MessageWriter<ProcessStackSpawnRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let result = match &request.command {
            ServiceAgentCommand::NewTerminalTab {
                cwd,
                command,
                args,
                env,
            } => match focus.pane.filter(|pane| panes.contains(*pane)) {
                None => AgentCommandResult::Error("no active pane".to_string()),
                Some(pane) => match valid_cwd(cwd) {
                    Err(message) => AgentCommandResult::Error(message),
                    Ok(cwd) => {
                        let activate = !request.origin.is_agent();
                        let cwd = cwd.or_else(|| {
                            active_space
                                .as_ref()
                                .and_then(|space| settings.startup_dir(&space.record.id))
                        });
                        if command.trim().is_empty() {
                            terminal_spawn.write(TerminalStackSpawnRequest {
                                pane,
                                cwd,
                                shell: None,
                                agent_run: false,
                                pending_input: None,
                                process_id: None,
                                activate,
                            });
                            AgentCommandResult::Ok
                        } else if let Some(cwd) = cwd {
                            process_spawn.write(ProcessStackSpawnRequest {
                                pane,
                                command: command.clone(),
                                args: args.clone(),
                                cwd,
                                env: env.clone(),
                                activate,
                            });
                            AgentCommandResult::Ok
                        } else {
                            AgentCommandResult::Error(
                                "project directory is required to run a command".to_string(),
                            )
                        }
                    }
                },
            },
            ServiceAgentCommand::RunShell { command, cwd, mode } => {
                let mode = match mode {
                    AgentShellMode::Active => vmux_terminal::ShellMode::Active,
                    AgentShellMode::NewTab => vmux_terminal::ShellMode::NewTab,
                };
                run_shell.write(vmux_terminal::RunShellRequest {
                    command: command.clone(),
                    cwd: cwd.clone(),
                    mode,
                });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::TerminalSend { text, terminal } => {
                terminal_send.write(vmux_terminal::TerminalSendRequest {
                    text: text.clone(),
                    terminal: terminal.clone(),
                });
                AgentCommandResult::Ok
            }
            _ => continue,
        };
        service_requests.write(ServiceRequest(request.response(result)));
    }
}

fn handle_browser_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    mut navigate: MessageWriter<vmux_layout::BrowserNavigateRequest>,
    mut go_back: MessageWriter<vmux_layout::BrowserGoBackRequest>,
    mut go_forward: MessageWriter<vmux_layout::BrowserGoForwardRequest>,
    mut open_stack: MessageWriter<vmux_layout::OpenInNewStackRequest>,
    mut install_extension: MessageWriter<vmux_layout::ExtensionInstallRequest>,
    mut open_beside: MessageWriter<vmux_layout::OpenBesideRequest>,
    mut activate: MessageWriter<vmux_layout::active_pane::ActivatePane>,
    browse: AgentBrowserResolve,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let result = match &request.command {
            ServiceAgentCommand::BrowserNavigate { url, pane } => {
                let mut pane = pane.clone();
                let mut new_stack = false;
                let mut profile = None;
                if pane.is_none()
                    && let CommandOrigin::Agent {
                        anchor: Some(anchor),
                        ..
                    } = &request.origin
                {
                    profile = Some(format!("{anchor:?}"));
                    if let Some(claim) = browse.claim_browser_pane(*anchor) {
                        pane = Some(claim.pane.to_bits().to_string());
                        new_stack = true;
                        activate.write(claim.activation);
                    } else if let Some(agent_pane) = browse.agent_pane(*anchor) {
                        open_beside.write(vmux_layout::OpenBesideRequest {
                            pane: agent_pane,
                            direction: None,
                            url: url.clone(),
                            request_id: request.request_id.0,
                            focus: false,
                        });
                        continue;
                    } else {
                        service_requests.write(ServiceRequest(request.response(
                            AgentCommandResult::Error(
                                "browser_navigate: agent has no resolvable pane".to_string(),
                            ),
                        )));
                        continue;
                    }
                }
                navigate.write(vmux_layout::BrowserNavigateRequest {
                    url: url.clone(),
                    pane,
                    request_id: Some(request.request_id.0),
                    new_stack,
                    profile,
                });
                continue;
            }
            ServiceAgentCommand::BrowserInstallExtension { source } => {
                install_extension.write(vmux_layout::ExtensionInstallRequest {
                    source: source.clone(),
                });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::BrowserGoBack { pane } => {
                let resolved = browse.command_pane(pane, &request.origin);
                if let Some(request) = resolved.activation {
                    activate.write(request);
                }
                go_back.write(vmux_layout::BrowserGoBackRequest {
                    pane: resolved.pane,
                });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::BrowserGoForward { pane } => {
                let resolved = browse.command_pane(pane, &request.origin);
                if let Some(request) = resolved.activation {
                    activate.write(request);
                }
                go_forward.write(vmux_layout::BrowserGoForwardRequest {
                    pane: resolved.pane,
                });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::BrowserHistorySearch { query, limit } => {
                bevy::log::info!("browser_history_search: query={:?} limit={}", query, limit);
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::OpenInNewStack { url } => {
                open_stack.write(vmux_layout::OpenInNewStackRequest { url: url.clone() });
                AgentCommandResult::Ok
            }
            _ => continue,
        };
        service_requests.write(ServiceRequest(request.response(result)));
    }
}

fn handle_desktop_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    agents: Query<(
        Entity,
        &vmux_core::team::Agent,
        Option<&vmux_service::protocol::ProcessId>,
    )>,
    user: Query<Entity, With<vmux_core::team::User>>,
    focus: Res<FocusedStack>,
    mut settings: ResMut<AppSettings>,
    mut settings_write: MessageWriter<vmux_setting::SettingsWriteRequest>,
    mut layout_apply: MessageWriter<vmux_layout::apply::LayoutApplyRequest>,
    mut focus_pane: MessageWriter<FocusPaneRequest>,
    mut rename_profile: MessageWriter<RenameProfileRequest>,
    mut attention: MessageWriter<vmux_core::notify::AgentAttention>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let result = match &request.command {
            ServiceAgentCommand::Notify { title, body } => {
                let caller = match &request.origin {
                    CommandOrigin::Agent {
                        anchor: Some(pid), ..
                    } => agents
                        .iter()
                        .find(|(_, _, process)| process.is_some_and(|process| process == pid))
                        .map(|(entity, _, _)| entity),
                    CommandOrigin::Agent { sid: Some(sid), .. } if !sid.is_empty() => agents
                        .iter()
                        .find(|(_, agent, _)| &agent.sid == sid)
                        .map(|(entity, _, _)| entity),
                    CommandOrigin::User => user.single().ok(),
                    _ => None,
                };
                match caller {
                    Some(caller) => {
                        attention.write(vmux_core::notify::AgentAttention {
                            entity: caller,
                            title: title.clone(),
                            body: body.clone(),
                        });
                        AgentCommandResult::Ok
                    }
                    None => AgentCommandResult::Error("notify: caller not found".to_string()),
                }
            }
            ServiceAgentCommand::FocusPane { pane } => {
                if request.origin.is_agent() {
                    AgentCommandResult::Error("focus_pane is disabled for agents".to_string())
                } else {
                    focus_pane.write(FocusPaneRequest { pane: pane.clone() });
                    AgentCommandResult::Ok
                }
            }
            ServiceAgentCommand::RenameProfile { name } => {
                rename_profile.write(RenameProfileRequest { name: name.clone() });
                AgentCommandResult::Ok
            }
            ServiceAgentCommand::UpdateSettings { path, value } => {
                match serde_json::Value::try_from(value) {
                    Ok(value) => {
                        let mut updated = (*settings).clone();
                        match updated.apply_update(path, value) {
                            Ok(ron_bytes) => {
                                if request.origin.is_agent()
                                    && updated.agent.allow_run_placement_override
                                        != settings.agent.allow_run_placement_override
                                {
                                    AgentCommandResult::Error(
                                        "update_settings: agent.allow_run_placement_override can only be changed in Settings"
                                            .to_string(),
                                    )
                                } else {
                                    *settings = updated;
                                    settings_write
                                        .write(vmux_setting::SettingsWriteRequest { ron_bytes });
                                    AgentCommandResult::Ok
                                }
                            }
                            Err(message) => AgentCommandResult::Error(message),
                        }
                    }
                    Err(error) => AgentCommandResult::Error(format!(
                        "update_settings: invalid JSON value: {error}"
                    )),
                }
            }
            ServiceAgentCommand::UpdateLayout { layout } => {
                let mut layout = layout.clone();
                if request.origin.is_agent() {
                    preserve_current_focus_in_layout_snapshot(&mut layout, &focus);
                }
                layout_apply.write(vmux_layout::apply::LayoutApplyRequest {
                    request_id: request.request_id.0,
                    snapshot: layout,
                });
                continue;
            }
            _ => continue,
        };
        service_requests.write(ServiceRequest(request.response(result)));
    }
}

fn handle_space_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    mut create_requests: MessageWriter<vmux_space::SpaceCreateRequest>,
    mut rename_requests: MessageWriter<vmux_space::SpaceRenameRequest>,
    mut delete_requests: MessageWriter<vmux_space::SpaceDeleteRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let result = match &request.command {
            ServiceAgentCommand::SpaceCommand(command) => {
                match command {
                    AgentSpaceCommand::Create { name } => {
                        create_requests.write(vmux_core::event::space::SpaceCreateRequest {
                            name: name.clone().unwrap_or_default(),
                        });
                    }
                    AgentSpaceCommand::Rename { space_id, name } => {
                        rename_requests.write(vmux_core::event::space::SpaceRenameRequest {
                            space_id: space_id.clone(),
                            name: name.clone(),
                        });
                    }
                    AgentSpaceCommand::Delete { space_id } => {
                        delete_requests.write(vmux_core::event::space::SpaceDeleteRequest {
                            space_id: space_id.clone(),
                        });
                    }
                }
                AgentCommandResult::Ok
            }
            _ => continue,
        };
        service_requests.write(ServiceRequest(request.response(result)));
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_bookmark_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    mut add_requests: MessageWriter<vmux_layout::bookmark::AddRequest>,
    mut remove_requests: MessageWriter<vmux_layout::bookmark::RemoveRequest>,
    mut pin_requests: MessageWriter<vmux_layout::bookmark::PinRequest>,
    mut pin_url_requests: MessageWriter<vmux_layout::bookmark::PinUrlRequest>,
    mut unpin_requests: MessageWriter<vmux_layout::bookmark::UnpinRequest>,
    mut create_folder_requests: MessageWriter<vmux_layout::bookmark::CreateFolderRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let ServiceAgentCommand::BookmarkCommand(command) = &request.command else {
            continue;
        };
        match command {
            AgentBookmarkCommand::Add { page, folder } => {
                add_requests.write(vmux_layout::bookmark::AddRequest {
                    metadata: vmux_core::PageMetadata {
                        title: page.title.clone().unwrap_or_default(),
                        url: page.url.clone(),
                        icon: vmux_core::PageIcon::favicon(
                            page.favicon_url.clone().unwrap_or_default(),
                        ),
                        bg_color: None,
                    },
                    folder: folder.clone(),
                });
            }
            AgentBookmarkCommand::Remove { uuid } => {
                remove_requests.write(vmux_layout::bookmark::RemoveRequest { uuid: uuid.clone() });
            }
            AgentBookmarkCommand::Pin { uuid } => {
                pin_requests.write(vmux_layout::bookmark::PinRequest { uuid: uuid.clone() });
            }
            AgentBookmarkCommand::PinUrl { page } => {
                pin_url_requests.write(vmux_layout::bookmark::PinUrlRequest {
                    metadata: vmux_core::PageMetadata {
                        title: page.title.clone().unwrap_or_default(),
                        url: page.url.clone(),
                        icon: vmux_core::PageIcon::favicon(
                            page.favicon_url.clone().unwrap_or_default(),
                        ),
                        bg_color: None,
                    },
                });
            }
            AgentBookmarkCommand::Unpin { uuid } => {
                unpin_requests.write(vmux_layout::bookmark::UnpinRequest { uuid: uuid.clone() });
            }
            AgentBookmarkCommand::CreateFolder { name } => {
                create_folder_requests.write(vmux_layout::bookmark::CreateFolderRequest::root(
                    name.clone(),
                ));
            }
        }
        service_requests.write(ServiceRequest(request.response(AgentCommandResult::Ok)));
    }
}

fn handle_shared_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    command_bar: Res<vmux_command::snapshot::CommandBarProjection>,
    contributed_pages: Query<&vmux_command::snapshot::ContributedPage>,
    mut new_tabs: MessageWriter<vmux_layout::NewTabRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let result = match &request.command {
            ServiceAgentCommand::Shared(SharedAgentCommand::NewAgentChat {
                prompt,
                agent_url,
                ..
            }) => match vmux_command::snapshot::ContributedPage::prompt_url(
                &contributed_pages,
                agent_url.as_deref(),
            ) {
                Some(url) => {
                    new_tabs.write(vmux_layout::NewTabRequest {
                        url,
                        pending_prompt: Some(prompt.clone()),
                    });
                    AgentCommandResult::Ok
                }
                None => AgentCommandResult::Error("no agent is installed".to_string()),
            },
            ServiceAgentCommand::Shared(SharedAgentCommand::ListAgents) => {
                match serde_json::to_string(&remote_agents(&command_bar.agents)) {
                    Ok(json) => AgentCommandResult::Text(json),
                    Err(error) => AgentCommandResult::Error(format!("list_agents: {error}")),
                }
            }
            _ => continue,
        };
        service_requests.write(ServiceRequest(request.response(result)));
    }
}

fn forward_history_open_intent(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::AgentSessionPlugin;
    use crate::host::test_support::test_settings;
    use vmux_service::protocol::ProcessId;
    use vmux_terminal::Terminal;

    #[derive(Resource, Default)]
    struct CapturedTerminalSends(Vec<vmux_terminal::TerminalSendRequest>);

    impl CapturedTerminalSends {
        fn capture(
            mut requests: MessageReader<vmux_terminal::TerminalSendRequest>,
            mut captured: ResMut<Self>,
        ) {
            captured.0.extend(requests.read().cloned());
        }
    }

    #[test]
    pub(crate) fn update_settings_via_apply_mutates_resource_and_returns_ron() {
        let mut settings = test_settings();
        let ron_bytes = settings
            .apply_update(
                "browser.startup_url",
                serde_json::json!("https://example.com/custom"),
            )
            .expect("apply ok");
        assert_eq!(settings.browser.startup_url, "https://example.com/custom");
        assert!(ron_bytes.contains("https://example.com/custom"));
    }

    #[test]
    pub(crate) fn run_placement_override_settings_update_rejects_agents_and_allows_users() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
        ))
        .add_message::<vmux_setting::SettingsWriteRequest>()
        .add_message::<vmux_space::SpaceCreateRequest>()
        .add_message::<vmux_space::SpaceRenameRequest>()
        .add_message::<vmux_space::SpaceDeleteRequest>()
        .add_message::<vmux_history::query::HistoryOpenIntent>()
        .insert_resource(FocusedStack::default())
        .insert_resource(test_settings());

        let mut agent_value = serde_json::to_value(vmux_setting::AgentSettings::default()).unwrap();
        agent_value["allow_run_placement_override"] = serde_json::json!(true);
        for (path, value) in [
            (
                "agent.allow_run_placement_override",
                vmux_api::json::JsonValue::Bool(true),
            ),
            ("agent", vmux_api::json::JsonValue::from(agent_value)),
        ] {
            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    origin: CommandOrigin::Agent {
                        sid: Some("test-agent".to_string()),
                        anchor: None,
                    },
                    command: ServiceAgentCommand::UpdateSettings {
                        path: path.to_string(),
                        value,
                    },
                });
            app.update();
            assert!(
                !app.world()
                    .resource::<AppSettings>()
                    .agent
                    .allow_run_placement_override,
                "agent update unexpectedly enabled override through {path}"
            );
        }

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::User,
                command: ServiceAgentCommand::UpdateSettings {
                    path: "agent.allow_run_placement_override".to_string(),
                    value: vmux_api::json::JsonValue::Bool(true),
                },
            });
        app.update();
        assert!(
            app.world()
                .resource::<AppSettings>()
                .agent
                .allow_run_placement_override
        );
    }

    #[test]
    pub(crate) fn terminal_send_writes_raw_text_to_active_terminal() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
            vmux_terminal::TerminalRequestPlugin,
        ))
        .add_message::<vmux_setting::SettingsWriteRequest>()
        .add_message::<vmux_space::SpaceCreateRequest>()
        .add_message::<vmux_space::SpaceRenameRequest>()
        .add_message::<vmux_space::SpaceDeleteRequest>()
        .add_message::<vmux_history::query::HistoryOpenIntent>()
        .init_resource::<CapturedTerminalSends>()
        .add_systems(
            Update,
            CapturedTerminalSends::capture.after(CommandSet::Commands),
        )
        .insert_resource(FocusedStack::default())
        .insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .insert(ChildOf(pane))
            .id();
        let _terminal = app
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

        let captured = &app.world().resource::<CapturedTerminalSends>().0;
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].text, "ls");
        assert_eq!(captured[0].terminal, None);
    }

    #[test]
    pub(crate) fn agent_origin_clears_requested_focus() {
        let origin = CommandOrigin::Agent {
            sid: Some("s1".into()),
            anchor: Some(ProcessId::new()),
        };

        assert!(!origin.allows_focus(true));
        assert!(!origin.allows_focus(false));
    }

    #[test]
    pub(crate) fn user_origin_keeps_requested_focus() {
        assert!(CommandOrigin::User.allows_focus(true));
        assert!(!CommandOrigin::User.allows_focus(false));
    }

    #[test]
    pub(crate) fn agent_layout_snapshot_keeps_current_focus() {
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
    fn command_arguments_reject_non_object_json() {
        assert!(vmux_core::JsonArguments::try_from(&vmux_api::json::JsonValue::Null).is_err());
        assert!(
            vmux_core::JsonArguments::try_from(&vmux_api::json::JsonValue::Array(Vec::new()))
                .is_err()
        );
        assert!(
            vmux_core::JsonArguments::try_from(&vmux_api::json::JsonValue::Number("x".to_string()))
                .is_err()
        );
        assert_eq!(
            vmux_core::JsonArguments::try_from(&vmux_api::json::JsonValue::from(
                serde_json::json!({ "url": "https://example.com" })
            ))
            .unwrap()
            .0,
            serde_json::json!({ "url": "https://example.com" })
        );
    }
}
