use std::path::PathBuf;

use bevy::prelude::*;
use vmux_command::{AppCommand, WriteAppCommands};
use vmux_layout::{
    pane::{Pane, PaneSplit},
    stack::FocusedStack,
};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{
    AgentCommand as ServiceAgentCommand, AgentRequestId, AgentShellMode, ClientMessage,
    SharedAgentCommand,
};
use vmux_setting::AppSettings;
use vmux_space::ActiveSpace;
use vmux_terminal::{ServiceMessageSet, TerminalStackSpawnRequest};

use crate::events::{AgentCommandRequest, AgentQueryRequest, AgentToolCallRequest, CommandOrigin};

use super::browser_pane::AgentBrowserResolve;
use super::valid_cwd;

pub(super) struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                forward_history_open_intent,
                handle_agent_tool_calls,
                handle_agent_commands,
            )
                .chain()
                .in_set(WriteAppCommands)
                .after(ServiceMessageSet),
        )
        .add_systems(
            Update,
            (handle_focus_pane_requests, handle_rename_profile_requests)
                .after(handle_agent_commands),
        );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct SettingsParams<'w> {
    settings: ResMut<'w, AppSettings>,
    writes: MessageWriter<'w, vmux_setting::SettingsWriteRequest>,
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

pub(crate) fn origin_is_agent(origin: &CommandOrigin) -> bool {
    matches!(origin, CommandOrigin::Agent { .. })
}

pub(crate) fn requested_focus_for_origin(origin: &CommandOrigin, requested: bool) -> bool {
    requested && !origin_is_agent(origin)
}

pub(crate) fn focused_id(
    kind: vmux_layout::protocol::NodeKind,
    entity: Option<Entity>,
) -> Option<String> {
    entity.map(|entity| vmux_layout::protocol::format_id(kind, entity.to_bits()))
}

pub(crate) fn preserve_current_focus_in_layout_snapshot(
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

pub(crate) fn agent_may_dispatch_app_command(command: &AppCommand) -> bool {
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
pub(crate) struct AgentSpaceWriters<'w, 's> {
    layout_apply: MessageWriter<'w, vmux_layout::apply::LayoutApplyRequest>,
    space_command: MessageWriter<'w, vmux_space::SpaceCommandRequest>,
    bookmark_op: MessageWriter<'w, vmux_layout::bookmark::BookmarkOp>,
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

pub(super) fn handle_agent_tool_calls(
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

pub(crate) fn remote_agents(
    snapshot: &vmux_command::snapshot::CommandBarAgentsSnapshot,
) -> Vec<vmux_wire::room::RemoteAgent> {
    snapshot
        .acp
        .iter()
        .map(|agent| vmux_wire::room::RemoteAgent {
            id: agent.id.clone(),
            name: agent.name.clone(),
            url: agent.url.clone(),
            icon: agent.icon.clone(),
        })
        .chain(
            snapshot
                .providers
                .iter()
                .map(|agent| vmux_wire::room::RemoteAgent {
                    id: agent.id.clone(),
                    name: format!("{} (CLI)", agent.name),
                    url: agent.url.clone(),
                    icon: agent.icon.clone(),
                }),
        )
        .collect()
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct DesktopContext<'w, 's> {
    focus: Res<'w, FocusedStack>,
    agents: Res<'w, vmux_command::snapshot::CommandBarAgentsSnapshot>,
    contributions: vmux_command::snapshot::Contributions<'w, 's>,
}

pub(super) fn handle_agent_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    mut app_commands: MessageWriter<AppCommand>,
    mut browser_nav_writer: MessageWriter<vmux_layout::BrowserNavigateRequest>,
    mut browser_go_back_writer: MessageWriter<vmux_layout::BrowserGoBackRequest>,
    mut browser_go_forward_writer: MessageWriter<vmux_layout::BrowserGoForwardRequest>,
    mut stack_writers: (
        MessageWriter<vmux_layout::OpenInNewStackRequest>,
        MessageWriter<vmux_layout::ExtensionInstallRequest>,
        MessageWriter<vmux_layout::NewTabRequest>,
    ),
    mut terminal_send_writer: MessageWriter<vmux_terminal::TerminalSendRequest>,
    mut run_shell_writer: MessageWriter<vmux_terminal::RunShellRequest>,
    mut terminal_stack_spawn_writer: MessageWriter<TerminalStackSpawnRequest>,
    mut process_stack_spawn_writer: MessageWriter<ProcessStackSpawnRequest>,
    desktop: DesktopContext,
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
            ServiceAgentCommand::FileSearch { .. } => AgentCommandResult::Ok,
            ServiceAgentCommand::TurnEnded { .. } => AgentCommandResult::Ok,
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
            } => match desktop.focus.pane.filter(|pane| panes.contains(*pane)) {
                None => AgentCommandResult::Error("no active pane".to_string()),
                Some(pane) => match valid_cwd(cwd) {
                    Err(message) => AgentCommandResult::Error(message),
                    Ok(cwd_opt) => {
                        let activate = !origin_is_agent(&request.origin);
                        let cwd_path = cwd_opt.or_else(|| {
                            active_space.as_ref().and_then(|space| {
                                vmux_setting::resolve_startup_dir(&sp.settings, &space.record.id)
                            })
                        });
                        if command.trim().is_empty() {
                            terminal_stack_spawn_writer.write(TerminalStackSpawnRequest {
                                pane,
                                cwd: cwd_path,
                                shell: None,
                                agent_run: false,
                                pending_input: None,
                                process_id: None,
                                activate,
                            });
                            AgentCommandResult::Ok
                        } else if let Some(cwd_path) = cwd_path {
                            process_stack_spawn_writer.write(ProcessStackSpawnRequest {
                                pane,
                                command: command.clone(),
                                args: args.clone(),
                                cwd: cwd_path,
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
                let mut new_stack = false;
                let mut profile = None;
                if pane.is_none()
                    && let CommandOrigin::Agent {
                        anchor: Some(anchor),
                        ..
                    } = &request.origin
                {
                    profile = Some(format!("{anchor:?}"));
                    if let Some((browser_pane, _)) = writers.browse.claim_browser_pane(*anchor) {
                        pane = Some(browser_pane.to_bits().to_string());
                        new_stack = true;
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
                    new_stack,
                    profile,
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
                        let mut updated = (*sp.settings).clone();
                        match vmux_setting::apply_settings_update(&mut updated, path, value) {
                            Ok(ron_bytes) => {
                                if origin_is_agent(&request.origin)
                                    && updated.agent.allow_run_placement_override
                                        != sp.settings.agent.allow_run_placement_override
                                {
                                    AgentCommandResult::Error(
                                        "update_settings: agent.allow_run_placement_override can only be changed in Settings"
                                            .to_string(),
                                    )
                                } else {
                                    *sp.settings = updated;
                                    sp.writes
                                        .write(vmux_setting::SettingsWriteRequest { ron_bytes });
                                    AgentCommandResult::Ok
                                }
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
                    preserve_current_focus_in_layout_snapshot(&mut layout, &desktop.focus);
                }
                writers
                    .layout_apply
                    .write(vmux_layout::apply::LayoutApplyRequest {
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
            ServiceAgentCommand::BookmarkCommand {
                command,
                uuid,
                name,
                url,
                title,
                favicon_url,
            } => {
                use vmux_layout::bookmark::BookmarkOp;
                let metadata = |url| vmux_core::PageMetadata {
                    title: title.clone().unwrap_or_default(),
                    url,
                    icon: vmux_core::PageIcon::favicon(favicon_url.clone().unwrap_or_default()),
                    bg_color: None,
                };
                let op = match command.as_str() {
                    "add" => url.clone().map(|url| BookmarkOp::Add {
                        metadata: metadata(url),
                        folder: uuid.clone(),
                    }),
                    "remove" => uuid.clone().map(|uuid| BookmarkOp::Remove { uuid }),
                    "pin" => match (uuid.clone(), url.clone()) {
                        (Some(uuid), _) => Some(BookmarkOp::Pin { uuid }),
                        (None, Some(url)) => Some(BookmarkOp::PinUrl {
                            metadata: metadata(url),
                        }),
                        _ => None,
                    },
                    "unpin" => uuid.clone().map(|uuid| BookmarkOp::Unpin { uuid }),
                    "folder_create" => name.clone().map(|name| BookmarkOp::AddFolder { name }),
                    _ => None,
                };
                match op {
                    Some(op) => {
                        writers.bookmark_op.write(op);
                        AgentCommandResult::Ok
                    }
                    None => AgentCommandResult::Error("invalid bookmark command".to_string()),
                }
            }
            ServiceAgentCommand::Shared(SharedAgentCommand::NewAgentChat {
                prompt,
                agent_url,
                ..
            }) => match desktop.contributions.prompt_url(agent_url.as_deref()) {
                Some(url) => {
                    stack_writers.2.write(vmux_layout::NewTabRequest {
                        url,
                        pending_prompt: Some(prompt.clone()),
                    });
                    AgentCommandResult::Ok
                }
                None => AgentCommandResult::Error("no agent is installed".to_string()),
            },
            ServiceAgentCommand::Shared(SharedAgentCommand::ListAgents) => {
                match serde_json::to_string(&remote_agents(&desktop.agents)) {
                    Ok(json) => AgentCommandResult::Text(json),
                    Err(error) => AgentCommandResult::Error(format!("list_agents: {error}")),
                }
            }
            ServiceAgentCommand::Shared(SharedAgentCommand::ListTeam)
            | ServiceAgentCommand::Shared(SharedAgentCommand::ListModels { .. })
            | ServiceAgentCommand::Shared(SharedAgentCommand::SelectModel { .. })
            | ServiceAgentCommand::Shared(SharedAgentCommand::SetEffort { .. })
            | ServiceAgentCommand::OpenBeside { .. }
            | ServiceAgentCommand::Run { .. }
            | ServiceAgentCommand::RunWithPlacementOverride { .. }
            | ServiceAgentCommand::CreateWorktree { .. }
            | ServiceAgentCommand::ChooseWorkspace { .. }
            | ServiceAgentCommand::ChooseWorkspaceAtPath { .. }
            | ServiceAgentCommand::PrepareWorktree { .. }
            | ServiceAgentCommand::RequestUserChoice { .. }
            | ServiceAgentCommand::SetConversationTitle { .. }
            | ServiceAgentCommand::SearchKnowledge { .. }
            | ServiceAgentCommand::ReadKnowledge { .. }
            | ServiceAgentCommand::WriteKnowledge { .. }
            | ServiceAgentCommand::CreateWorktreeOnBranch { .. }
            | ServiceAgentCommand::ResumeInAcp { .. } => {
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

    #[test]
    pub(crate) fn update_settings_via_apply_mutates_resource_and_returns_ron() {
        let mut settings = test_settings();
        let ron_bytes = vmux_setting::apply_settings_update(
            &mut settings,
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
        .add_message::<vmux_space::SpaceCommandRequest>()
        .add_message::<vmux_history::query::HistoryOpenIntent>()
        .insert_resource(FocusedStack::default())
        .insert_resource(test_settings());

        let mut agent_value = serde_json::to_value(vmux_setting::AgentSettings::default()).unwrap();
        agent_value["allow_run_placement_override"] = serde_json::json!(true);
        for (path, value_json) in [
            (
                "agent.allow_run_placement_override",
                serde_json::json!(true).to_string(),
            ),
            ("agent", agent_value.to_string()),
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
                        value_json,
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
                    value_json: serde_json::json!(true).to_string(),
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
        ))
        .add_message::<vmux_setting::SettingsWriteRequest>()
        .add_message::<vmux_space::SpaceCommandRequest>()
        .add_message::<vmux_history::query::HistoryOpenIntent>()
        .add_systems(Update, vmux_terminal::handle_terminal_send_requests)
        .insert_resource(FocusedStack::default())
        .insert_resource(test_settings());

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
    pub(crate) fn agent_origin_clears_requested_focus() {
        let origin = CommandOrigin::Agent {
            sid: Some("s1".into()),
            anchor: Some(ProcessId::new()),
        };

        assert!(!requested_focus_for_origin(&origin, true));
        assert!(!requested_focus_for_origin(&origin, false));
    }

    #[test]
    pub(crate) fn user_origin_keeps_requested_focus() {
        assert!(requested_focus_for_origin(&CommandOrigin::User, true));
        assert!(!requested_focus_for_origin(&CommandOrigin::User, false));
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
    pub(crate) fn agent_app_command_filter_blocks_focus_changers() {
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
}
