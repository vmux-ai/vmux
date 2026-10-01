use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_core::PageMetadata;

pub(super) struct AttachPlugin;

impl Plugin for AttachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, attach);
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

fn attach(
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
        let agent_id = agent_id.as_str();
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
            vmux_core::agent::AgentSessionRoot,
            vmux_session::AcpSession {
                agent_id: agent_id.to_string(),
                sid: sid.clone(),
                cwd: cwd.clone(),
                anchor,
                resume: resume.clone(),
            },
            vmux_core::team::Profile::registry(name, agent_id),
            vmux_core::team::Agent { sid: sid.clone() },
            vmux_core::AgentWorkingDir(cwd.to_string_lossy().to_string()),
        ));
        let view = if let Some(webview) = *webview {
            webview
        } else {
            commands
                .spawn((
                    vmux_layout::Browser::native_page(&url, name),
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
    catalog: Option<&'a crate::host::runtime::AcpCatalog>,
    id: &str,
) -> Option<&'a crate::host::acp::registry::RegistryAgent> {
    catalog?.agents.iter().find(|agent| agent.id == id)
}

pub(crate) fn acp_icon_for_id(
    catalog: Option<&crate::host::runtime::AcpCatalog>,
    id: &str,
) -> Option<String> {
    acp_registry_agent_for_id(catalog, id).and_then(|agent| agent.icon.clone())
}

pub(crate) fn acp_profile_name_for_id(
    id: &str,
    config: Option<&vmux_setting::AcpAgentConfig>,
    catalog: Option<&crate::host::runtime::AcpCatalog>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_attach_gives_profile_agent_and_icon() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AttachPlugin));
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
        let meta = world.get::<PageMetadata>(stack).expect("meta");
        assert_eq!(meta.icon.favicon_url(), "https://cdn.example/vibe.svg");
    }

    #[test]
    fn acp_icon_for_id_reads_catalog() {
        use crate::host::acp::registry::{Distribution, RegistryAgent};
        let catalog = crate::host::runtime::AcpCatalog {
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
            acp_icon_for_id(Some(&catalog), "claude-acp").as_deref(),
            Some("https://cdn.example/claude.svg")
        );
        assert_eq!(acp_icon_for_id(Some(&catalog), "absent"), None);
        assert_eq!(acp_icon_for_id(None, "mistral-vibe"), None);
    }

    #[test]
    fn acp_profile_name_prefers_registry_then_config_then_id() {
        use crate::host::acp::registry::{Distribution, RegistryAgent};
        use vmux_setting::AcpAgentConfig;

        let mut config = AcpAgentConfig {
            id: "claude-acp".into(),
            name: "Configured Claude".into(),
            command: "npx".into(),
            args: vec![],
            env: vec![],
            cwd: None,
            version: None,
        };
        let catalog = crate::host::runtime::AcpCatalog {
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
            "claude-acp"
        );
    }
}
