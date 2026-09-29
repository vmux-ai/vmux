use std::path::PathBuf;

use bevy::prelude::*;
#[cfg(test)]
use vmux_api::protocol::AgentRequest;
use vmux_api::protocol::{AgentCommandResult, AgentResumeInAcp, ClientMessage, ProcessId};
use vmux_command::WriteCommandRequests;
use vmux_core::PageMetadata;
use vmux_core::agent::AgentKind;
use vmux_core::service::{ServiceMessageSet, ServiceRequest};
use vmux_setting::AppSettings;
use vmux_terminal::Terminal;
use vmux_terminal::launch::TerminalLaunch;

use crate::AgentVariant;
use crate::event::{AgentRequestInput, CommandOrigin};
use crate::runtime::strategy::{Strategy, StrategyKey, StrategyKind};
use crate::session::{AgentSession, SessionId};

pub(super) struct AttachPlugin;

impl Plugin for AttachPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_systems(Update, (attach_page_agents, attach_acp_agents))
            .add_systems(
                Update,
                handle_resume_in_acp
                    .in_set(WriteCommandRequests)
                    .after(ServiceMessageSet)
                    .after(super::command::CommandSet::ToolCalls)
                    .before(super::command::CommandSet::Commands),
            );
    }
}

#[derive(Component)]
pub(super) struct PageAgentAttachment {
    kind: AgentKind,
    provider: String,
    model: String,
    sid: String,
    webview: Option<Entity>,
}

impl PageAgentAttachment {
    pub(super) fn new(
        kind: AgentKind,
        provider: impl Into<String>,
        model: impl Into<String>,
        sid: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            provider: provider.into(),
            model: model.into(),
            sid: sid.into(),
            webview: None,
        }
    }

    pub(super) fn with_webview(mut self, webview: Option<Entity>) -> Self {
        self.webview = webview;
        self
    }
}

#[derive(Component)]
pub(super) struct AcpAgentAttachment {
    agent_id: String,
    name: String,
    sid: String,
    cwd: std::path::PathBuf,
    icon: Option<String>,
    resume: Option<String>,
    webview: Option<Entity>,
}

impl AcpAgentAttachment {
    pub(super) fn new(
        agent_id: impl Into<String>,
        name: impl Into<String>,
        sid: impl Into<String>,
        cwd: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            name: name.into(),
            sid: sid.into(),
            cwd: cwd.into(),
            icon: None,
            resume: None,
            webview: None,
        }
    }

    pub(super) fn icon(mut self, icon: Option<String>) -> Self {
        self.icon = icon;
        self
    }

    pub(super) fn resume(mut self, resume: Option<String>) -> Self {
        self.resume = resume;
        self
    }

    pub(super) fn webview(mut self, webview: Option<Entity>) -> Self {
        self.webview = webview;
        self
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AgentStrategies<'w, 's> {
    strategies: Query<'w, 's, (&'static StrategyKey, &'static StrategyKind), With<Strategy>>,
}

impl AgentStrategies<'_, '_> {
    pub(super) fn page_kind(&self, provider: &str, model: &str) -> Result<AgentKind, String> {
        let Some((_, kind)) = self
            .strategies
            .iter()
            .find(|(key, _)| key.provider == provider && key.model == model)
        else {
            return Err(format!(
                "no Page agent strategy registered for {}/{}",
                provider, model
            ));
        };
        Ok(kind.0)
    }
}

fn attach_page_agents(
    attachments: Query<(Entity, &PageAgentAttachment), Added<PageAgentAttachment>>,
    mut commands: Commands,
) {
    for (entity, attachment) in &attachments {
        let PageAgentAttachment {
            kind,
            provider,
            model,
            sid,
            webview,
        } = attachment;
        let title = format!("{provider}/{model}");
        let url = format!("{}{}", crate::url::page_url_prefix(&provider, &model), sid);
        commands.entity(entity).insert(PageMetadata {
            url: url.clone(),
            title: title.clone(),
            bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
            ..default()
        });
        commands.entity(entity).insert((
            vmux_session::AgentSession {
                kind: *kind,
                variant: AgentVariant::Page,
                sid: sid.clone(),
                provider: provider.clone(),
                model: model.clone(),
            },
            crate::AgentMessages::default(),
            crate::AgentApprovalPolicy::default(),
            vmux_session::AgentRunState::default(),
            vmux_core::team::Profile::agent(*kind),
            vmux_core::team::Agent {
                sid: sid.clone(),
                kind: Some(*kind),
            },
        ));
        let url = format!("vmux://sessions/{provider}");
        if let Some(webview) = *webview {
            commands
                .entity(webview)
                .insert((
                    PageMetadata {
                        url,
                        title,
                        bg_color: None,
                        ..default()
                    },
                    vmux_chat::host::ChatView,
                ))
                .remove::<(
                    vmux_start::StartInlineTransitionView,
                    vmux_core::launcher::HostsLauncher,
                    vmux_core::page::PageReady,
                )>();
        } else {
            commands.spawn((
                vmux_layout::Browser::native_page(&url, &title),
                vmux_chat::host::ChatView,
                ChildOf(entity),
            ));
        }
        commands.entity(entity).remove::<PageAgentAttachment>();
    }
}

fn attach_acp_agents(
    attachments: Query<(Entity, &AcpAgentAttachment), Added<AcpAgentAttachment>>,
    mut commands: Commands,
) {
    for (entity, request) in &attachments {
        let AcpAgentAttachment {
            agent_id,
            name,
            sid,
            cwd,
            icon,
            resume,
            webview,
        } = request;
        let agent_id = crate::acp_tool::agent_url_id(agent_id);
        let url = match resume.as_deref() {
            Some(acp_sid) => format!("vmux://sessions/{agent_id}/{acp_sid}"),
            None => format!("vmux://sessions/{agent_id}"),
        };
        let favicon = vmux_core::PageIcon::favicon(icon.as_deref().unwrap_or(""));
        commands.entity(entity).insert(PageMetadata {
            url: url.clone(),
            title: name.clone(),
            bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
            icon: favicon.clone(),
        });
        let anchor = ProcessId::new();
        commands.entity(entity).insert((
            vmux_session::AcpSession {
                agent_id: agent_id.to_string(),
                sid: sid.clone(),
                cwd: cwd.clone(),
                anchor,
                resume: resume.clone(),
            },
            crate::AgentMessages::default(),
            crate::AgentApprovalPolicy::default(),
            vmux_session::AgentRunState::default(),
            vmux_core::team::Profile::registry(&name, agent_id),
            vmux_core::team::Agent {
                sid: sid.clone(),
                kind: None,
            },
            vmux_core::AgentWorkingDir(cwd.to_string_lossy().to_string()),
        ));
        if let Some(resume) = resume.as_deref()
            && let Some(imported) = crate::handoff::load(agent_id, resume)
        {
            commands.entity(entity).insert(imported);
        }
        let view = if let Some(webview) = *webview {
            webview
        } else {
            commands
                .spawn((
                    vmux_layout::Browser::native_page(&url, &name),
                    vmux_chat::host::ChatView,
                    ChildOf(entity),
                    anchor,
                ))
                .id()
        };
        commands.entity(view).insert((
            PageMetadata {
                url,
                title: name.clone(),
                bg_color: None,
                icon: favicon,
            },
            anchor,
            vmux_chat::host::ChatView,
        ));
        if webview.is_some() {
            commands.entity(view).remove::<(
                vmux_start::StartInlineTransitionView,
                vmux_core::launcher::HostsLauncher,
                vmux_core::page::PageReady,
            )>();
        }
        commands.entity(entity).remove::<AcpAgentAttachment>();
    }
}

pub(crate) fn acp_registry_agent_for_id<'a>(
    catalog: Option<&'a crate::runtime::acp::AcpCatalog>,
    id: &str,
) -> Option<&'a crate::acp_registry::RegistryAgent> {
    catalog?
        .agents
        .iter()
        .find(|agent| crate::acp_tool::agent_ids_match(&agent.id, id))
}

pub(crate) fn acp_icon_for_id(
    catalog: Option<&crate::runtime::acp::AcpCatalog>,
    id: &str,
) -> Option<String> {
    acp_registry_agent_for_id(catalog, id).and_then(|agent| agent.icon.clone())
}

pub(crate) fn acp_profile_name_for_id(
    id: &str,
    config: Option<&vmux_setting::AcpAgentConfig>,
    catalog: Option<&crate::runtime::acp::AcpCatalog>,
) -> String {
    acp_registry_agent_for_id(catalog, id)
        .map(|agent| agent.name.trim())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            let name = config?.name.trim();
            (!name.is_empty()).then_some(name)
        })
        .unwrap_or(id)
        .to_string()
}

fn acp_target_id_for_kind(
    kind: AgentKind,
    configs: &[vmux_setting::AcpAgentConfig],
    catalog: Option<&crate::runtime::acp::AcpCatalog>,
) -> Option<String> {
    configs
        .iter()
        .find(|config| crate::session_source::acp_agent_kind(&config.id) == Some(kind))
        .map(|config| config.id.clone())
        .or_else(|| {
            let id = kind.as_url_segment();
            acp_registry_agent_for_id(catalog, id)
                .is_some()
                .then(|| id.to_string())
        })
}

fn handle_resume_in_acp(
    mut reader: MessageReader<AgentRequestInput>,
    cli_sessions: Query<
        (
            &ProcessId,
            &ChildOf,
            &AgentSession,
            Option<&SessionId>,
            &TerminalLaunch,
        ),
        With<Terminal>,
    >,
    settings: Res<AppSettings>,
    catalog: Option<Single<&crate::runtime::acp::AcpCatalog>>,
    mut swap: MessageWriter<vmux_core::agent::SwapStackSession>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let catalog = catalog.as_ref().map(|catalog| **catalog);
    for request in reader.read() {
        let Ok(Some(command)) = request.decode::<AgentResumeInAcp>() else {
            continue;
        };
        let anchor = &command.anchor;
        let result = if !matches!(
            &request.origin,
            CommandOrigin::Agent {
                anchor: Some(origin_anchor),
                ..
            } if origin_anchor == anchor
        ) {
            AgentCommandResult::Error("resume_in_acp: caller anchor mismatch".to_string())
        } else if let Some((_, child_of, session, session_id, launch)) = cli_sessions
            .iter()
            .find(|(process_id, ..)| *process_id == anchor)
        {
            if !crate::session_source::kind_supports_cross_runtime(session.kind) {
                AgentCommandResult::Error(format!(
                    "resume_in_acp: {} does not support ACP resume",
                    session.kind.display_name()
                ))
            } else if let Some(session_id) = session_id {
                if let Some(agent_id) =
                    acp_target_id_for_kind(session.kind, &settings.agent.acp, catalog)
                {
                    swap.write(vmux_core::agent::SwapStackSession {
                        stack: child_of.parent(),
                        target_url: crate::AgentUrl::Acp {
                            id: agent_id,
                            sid: Some(session_id.0.clone()),
                        }
                        .format(),
                        cwd: PathBuf::from(&launch.cwd),
                        handoff: None,
                    });
                    AgentCommandResult::Ok
                } else {
                    AgentCommandResult::Error(format!(
                        "resume_in_acp: no ACP runtime available for {}",
                        session.kind.display_name()
                    ))
                }
            } else {
                AgentCommandResult::Error(
                    "resume_in_acp: current CLI session id is not available yet".to_string(),
                )
            }
        } else {
            AgentCommandResult::Error("resume_in_acp: current CLI session not found".to_string())
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: request.request_id,
            result,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::test_support::test_settings;
    use vmux_api::protocol::AgentRequestId;
    use vmux_terminal::Terminal;

    #[test]
    fn acp_attach_gives_profile_agent_and_icon() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AttachPlugin))
            .insert_resource(test_settings())
            .add_message::<AgentRequestInput>()
            .add_message::<vmux_core::agent::SwapStackSession>();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().entity_mut(stack).insert(
            AcpAgentAttachment::new(
                "mistral-vibe",
                "Mistral Vibe",
                "sid-1",
                std::path::PathBuf::from("/tmp"),
            )
            .icon(Some("https://cdn.example/vibe.svg".to_string())),
        );
        app.update();

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
        let catalog = crate::runtime::acp::AcpCatalog {
            agents: vec![
                RegistryAgent {
                    id: "mistral-vibe".to_string(),
                    name: "Mistral Vibe".to_string(),
                    version: None,
                    description: None,
                    icon: Some("https://cdn.example/vibe.svg".to_string()),
                    repository: None,
                    distribution: Distribution::default(),
                },
                RegistryAgent {
                    id: "claude-acp".to_string(),
                    name: "Claude Agent".to_string(),
                    version: None,
                    description: None,
                    icon: Some("https://cdn.example/claude.svg".to_string()),
                    repository: None,
                    distribution: Distribution::default(),
                },
            ],
        };
        assert_eq!(
            acp_icon_for_id(Some(&catalog), "mistral-vibe").as_deref(),
            Some("https://cdn.example/vibe.svg")
        );
        assert_eq!(
            acp_icon_for_id(Some(&catalog), "claude").as_deref(),
            Some("https://cdn.example/claude.svg")
        );
        assert_eq!(acp_icon_for_id(Some(&catalog), "absent"), None);
        assert_eq!(acp_icon_for_id(None, "mistral-vibe"), None);
    }

    #[test]
    fn acp_profile_name_prefers_registry_then_config_then_id() {
        use crate::acp_registry::{Distribution, RegistryAgent};
        use vmux_setting::AcpAgentConfig;

        let mut config = AcpAgentConfig {
            id: "claude".into(),
            name: "Configured Claude".into(),
            command: "npx".into(),
            args: vec![],
            env: vec![],
            cwd: None,
            version: None,
        };
        let catalog = crate::runtime::acp::AcpCatalog {
            agents: vec![RegistryAgent {
                id: "claude-acp".into(),
                name: "Claude".into(),
                version: None,
                description: None,
                icon: None,
                repository: None,
                distribution: Distribution::default(),
            }],
        };

        assert_eq!(
            acp_profile_name_for_id(&config.id, Some(&config), Some(&catalog)),
            "Claude"
        );
        assert_eq!(
            acp_profile_name_for_id(&config.id, Some(&config), None),
            "Configured Claude"
        );
        config.name = "   ".into();
        assert_eq!(
            acp_profile_name_for_id(&config.id, Some(&config), None),
            "claude"
        );
    }

    #[test]
    fn acp_target_id_accepts_registry_alias_config() {
        let config = vmux_setting::AcpAgentConfig {
            id: "claude-acp".into(),
            name: "Claude".into(),
            command: "npx".into(),
            args: vec![],
            env: vec![],
            cwd: None,
            version: None,
        };

        assert_eq!(
            acp_target_id_for_kind(AgentKind::Claude, &[config], None).as_deref(),
            Some("claude-acp")
        );
    }

    #[test]
    fn resume_in_acp_command_swaps_current_cli_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AgentRequestInput>()
            .add_message::<ServiceRequest>()
            .add_message::<vmux_core::agent::SwapStackSession>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_resume_in_acp);
        let stack = app.world_mut().spawn_empty().id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((
            Terminal,
            anchor,
            ChildOf(stack),
            AgentSession {
                kind: AgentKind::Claude,
            },
            SessionId("session-7".into()),
            TerminalLaunch {
                command: "claude".into(),
                args: vec![],
                cwd: "/workspace/project".into(),
                env: vec![],
                kind: vmux_terminal::launch::TerminalKind::Claude,
            },
        ));
        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                request: AgentRequest::encode(&AgentResumeInAcp { anchor }).unwrap(),
            });

        app.update();

        let swaps: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_core::agent::SwapStackSession>>()
            .drain()
            .collect();
        assert_eq!(swaps.len(), 1);
        assert_eq!(swaps[0].stack, stack);
        assert_eq!(swaps[0].target_url, "vmux://sessions/claude/session-7");
        assert_eq!(swaps[0].cwd, PathBuf::from("/workspace/project"));
        assert!(swaps[0].handoff.is_none());
    }
}
