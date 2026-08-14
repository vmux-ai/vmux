//! Binding an agent to a stack: the ACP and in-page variants, and moving a CLI session onto ACP.
//!
//! Attaching is where a bare stack becomes an agent — it gains the session component, the team
//! profile, and the chat webview that renders it. The registry lookups answer the two questions
//! attaching asks of a bare agent id: what to call it, and which icon to show.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::WebviewExtendStandardMaterial;
use vmux_command::WriteAppCommands;
use vmux_core::PageMetadata;
use vmux_core::agent::AgentKind;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{
    AgentCommand as ServiceAgentCommand, AgentCommandResult, ClientMessage, ProcessId,
};
use vmux_setting::AppSettings;
use vmux_terminal::ServiceMessageSet;
use vmux_terminal::Terminal;
use vmux_terminal::launch::TerminalLaunch;

use crate::AgentVariant;
use crate::events::{AgentCommandRequest, CommandOrigin};
use crate::session::{AgentSession, SessionId};

pub(super) struct AttachPlugin;

impl Plugin for AttachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            handle_resume_in_acp
                .in_set(WriteAppCommands)
                .after(ServiceMessageSet)
                .after(super::command::handle_agent_tool_calls)
                .before(super::command::handle_agent_commands),
        );
    }
}

pub fn attach_page_agent_to_stack(
    stack: Entity,
    provider: &str,
    model: &str,
    sid: &str,
    commands: &mut Commands,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    idx: &crate::client::page::strategy_index::PageStrategyIndex,
    kind_q: &Query<&crate::client::page::strategy_components::StrategyKind>,
) -> Option<()> {
    attach_page_agent_to_stack_with_webview(
        stack, provider, model, sid, None, commands, webview_mt, idx, kind_q,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn attach_page_agent_to_stack_with_webview(
    stack: Entity,
    provider: &str,
    model: &str,
    sid: &str,
    webview: Option<Entity>,
    commands: &mut Commands,
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
        vmux_session::AgentSession {
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
    if let Some(webview) = webview {
        commands
            .entity(webview)
            .insert((
                crate::host::chat::AgentChatView,
                PageMetadata {
                    url,
                    title: format!("{provider}/{model}"),
                    bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
                    ..default()
                },
            ))
            .remove::<crate::host::chat::ChatSynced>();
    } else {
        commands.spawn((
            vmux_layout::Browser::new(webview_mt, &url),
            crate::host::chat::AgentChatView,
            ChildOf(stack),
        ));
    }
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
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    attach_acp_agent_to_stack_with_webview(
        stack, agent_id, name, sid, cwd, icon, resume, None, commands, webview_mt,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn attach_acp_agent_to_stack_with_webview(
    stack: Entity,
    agent_id: &str,
    name: &str,
    sid: &str,
    cwd: &std::path::Path,
    icon: Option<&str>,
    resume: Option<&str>,
    webview: Option<Entity>,
    commands: &mut Commands,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let agent_id = crate::acp_install::agent_url_id(agent_id);
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
        vmux_session::AcpSession {
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
    if let Some(resume) = resume
        && let Some(imported) = crate::handoff::load(agent_id, resume)
    {
        commands.entity(stack).insert(imported);
    }
    // The webview carries the anchor `ProcessId`, so vmux_mcp tool calls resolve to this pane.
    if let Some(webview) = webview {
        commands
            .entity(webview)
            .insert((
                crate::host::chat::AgentChatView,
                PageMetadata {
                    url,
                    title: name.to_string(),
                    bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
                    icon: vmux_core::PageIcon::favicon(icon.unwrap_or("")),
                },
                anchor,
            ))
            .remove::<crate::host::chat::ChatSynced>();
    } else {
        commands.spawn((
            vmux_layout::Browser::new(webview_mt, &url),
            crate::host::chat::AgentChatView,
            ChildOf(stack),
            anchor,
        ));
    }
}

/// The registry icon URL for an ACP agent id, if the catalog is loaded and lists it.
pub(crate) fn acp_registry_agent_for_id<'a>(
    catalog: Option<&'a crate::client::acp::AcpCatalog>,
    id: &str,
) -> Option<&'a crate::acp_registry::RegistryAgent> {
    catalog?
        .agents
        .iter()
        .find(|agent| crate::acp_install::agent_ids_match(&agent.id, id))
}

pub(crate) fn acp_icon_for_id(
    catalog: Option<&crate::client::acp::AcpCatalog>,
    id: &str,
) -> Option<String> {
    acp_registry_agent_for_id(catalog, id).and_then(|agent| agent.icon.clone())
}

pub(crate) fn acp_profile_name_for_id(
    id: &str,
    config: Option<&vmux_setting::AcpAgentConfig>,
    catalog: Option<&crate::client::acp::AcpCatalog>,
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
    catalog: Option<&crate::client::acp::AcpCatalog>,
) -> Option<String> {
    configs
        .iter()
        .find(|config| crate::strategy::acp_agent_kind(&config.id) == Some(kind))
        .map(|config| config.id.clone())
        .or_else(|| {
            let id = kind.as_url_segment();
            acp_registry_agent_for_id(catalog, id)
                .is_some()
                .then(|| id.to_string())
        })
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

fn handle_resume_in_acp(
    mut reader: MessageReader<AgentCommandRequest>,
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
    catalog: Option<Res<crate::client::acp::AcpCatalog>>,
    mut swap: MessageWriter<vmux_core::agent::SwapStackSession>,
    service: Option<Res<ServiceClient>>,
) {
    for request in reader.read() {
        let ServiceAgentCommand::ResumeInAcp { anchor } = &request.command else {
            continue;
        };
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
            if !crate::strategy::kind_supports_cross_runtime(session.kind) {
                AgentCommandResult::Error(format!(
                    "resume_in_acp: {} does not support ACP resume",
                    session.kind.display_name()
                ))
            } else if let Some(session_id) = session_id {
                if let Some(agent_id) =
                    acp_target_id_for_kind(session.kind, &settings.agent.acp, catalog.as_deref())
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
        if let Some(service) = service.as_ref() {
            service.0.send(ClientMessage::AgentCommandResponse {
                request_id: request.request_id,
                result,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::test_support::test_settings;
    use vmux_service::protocol::AgentRequestId;
    use vmux_terminal::Terminal;

    #[test]
    pub(crate) fn acp_attach_gives_profile_agent_and_icon() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();
        let stack = app.world_mut().spawn_empty().id();

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
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
    pub(crate) fn acp_icon_for_id_reads_catalog() {
        use crate::acp_registry::{Distribution, RegistryAgent};
        let catalog = crate::client::acp::AcpCatalog {
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
    pub(crate) fn acp_profile_name_prefers_registry_then_config_then_id() {
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
        let catalog = crate::client::acp::AcpCatalog {
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
    pub(crate) fn acp_target_id_accepts_registry_alias_config() {
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
    pub(crate) fn resume_in_acp_command_swaps_current_cli_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AgentCommandRequest>()
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
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                command: ServiceAgentCommand::ResumeInAcp { anchor },
            });

        app.update();

        let swaps: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_core::agent::SwapStackSession>>()
            .drain()
            .collect();
        assert_eq!(swaps.len(), 1);
        assert_eq!(swaps[0].stack, stack);
        assert_eq!(swaps[0].target_url, "vmux://agent/claude/session-7");
        assert_eq!(swaps[0].cwd, PathBuf::from("/workspace/project"));
        assert!(swaps[0].handoff.is_none());
    }
}
