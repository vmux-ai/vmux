use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_ecs::agent::SessionId;
use vmux_ecs::{AgentWorkingDir, EntityTarget, PageIcon, PageMetadata};

pub(super) struct AttachPlugin;

impl Plugin for AttachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, attach);
    }
}

#[derive(Bundle)]
pub(super) struct AcpAgentAttachment {
    pending: PendingAcpAgentAttachment,
    name: Name,
    agent_id: AcpAgentId,
    session_id: SessionId,
    working_directory: AgentWorkingDir,
    icon: AcpAttachmentIcon,
    resume: AcpResume,
}

impl AcpAgentAttachment {
    pub(super) fn new(
        agent_id: impl Into<String>,
        name: impl Into<String>,
        sid: impl Into<String>,
        cwd: impl Into<std::path::PathBuf>,
        icon: Option<String>,
        resume: Option<String>,
    ) -> Self {
        let name = name.into();
        Self {
            pending: PendingAcpAgentAttachment,
            name: Name::new(name),
            agent_id: AcpAgentId(agent_id.into()),
            session_id: SessionId(sid.into()),
            working_directory: AgentWorkingDir(cwd.into()),
            icon: AcpAttachmentIcon(PageIcon::favicon(icon.unwrap_or_default())),
            resume: AcpResume(resume),
        }
    }
}

#[derive(Component)]
struct PendingAcpAgentAttachment;

#[derive(Component)]
struct AcpAgentId(String);

#[derive(Component)]
struct AcpAttachmentIcon(PageIcon);

#[derive(Component)]
struct AcpResume(Option<String>);

fn attach(
    attachments: Query<
        (
            Entity,
            &Name,
            &AcpAgentId,
            &SessionId,
            &AgentWorkingDir,
            &AcpAttachmentIcon,
            &AcpResume,
            Option<&EntityTarget<vmux_chat::host::ChatView>>,
        ),
        Added<PendingAcpAgentAttachment>,
    >,
    mut commands: Commands,
) {
    for (
        entity,
        name,
        AcpAgentId(agent_id),
        SessionId(sid),
        AgentWorkingDir(cwd),
        AcpAttachmentIcon(icon),
        AcpResume(resume),
        webview,
    ) in &attachments
    {
        let agent_id = agent_id.as_str();
        let name = name.as_str();
        let url = match resume.as_deref() {
            Some(acp_sid) => format!("{}{agent_id}/{acp_sid}", vmux_chat::ChatPlugin::URL),
            None => format!("{}{agent_id}", vmux_chat::ChatPlugin::URL),
        };
        commands.entity(entity).insert(PageMetadata {
            url: url.clone(),
            title: name.to_string(),
            bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
            icon: icon.clone(),
        });
        let anchor = ProcessId::new();
        commands.entity(entity).insert((
            vmux_ecs::agent::AgentSessionRoot,
            vmux_session::AcpSession {
                agent_id: agent_id.to_string(),
                sid: sid.clone(),
                cwd: cwd.clone(),
                anchor,
                resume: resume.clone(),
            },
            vmux_ecs::team::Profile::registry(name, agent_id),
            vmux_ecs::team::Agent { sid: sid.clone() },
        ));
        let view = if let Some(webview) = webview {
            webview.entity()
        } else {
            commands
                .spawn((
                    vmux_layout::Browser::native_page_with_icon(&url, name, icon.clone()),
                    vmux_chat::host::ChatView,
                    ChildOf(entity),
                    anchor,
                ))
                .id()
        };
        commands.entity(view).insert((
            ChildOf(entity),
            PageMetadata {
                url,
                title: name.to_string(),
                bg_color: None,
                icon: icon.clone(),
            },
            anchor,
            vmux_chat::host::ChatView,
        ));
        if webview.is_some() {
            commands.entity(view).remove::<(
                vmux_start::StartInlineTransitionView,
                vmux_ecs::launcher::HostsLauncher,
                vmux_ecs::page::PageReady,
            )>();
        }
        commands.entity(entity).remove::<(
            PendingAcpAgentAttachment,
            AcpAgentId,
            AcpAttachmentIcon,
            AcpResume,
            EntityTarget<vmux_chat::host::ChatView>,
        )>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_attach_gives_profile_agent_and_icon() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AttachPlugin));
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut()
            .entity_mut(stack)
            .insert(AcpAgentAttachment::new(
                "mistral-vibe",
                "Mistral Vibe",
                "sid-1",
                std::path::PathBuf::from("/tmp"),
                Some("https://cdn.example/vibe.svg".to_string()),
                None,
            ));
        app.update();

        let world = app.world();
        let profile = world
            .get::<vmux_ecs::team::Profile>(stack)
            .expect("profile");
        assert_eq!(profile.name, "Mistral Vibe");
        let agent = world.get::<vmux_ecs::team::Agent>(stack).expect("agent");
        assert_eq!(agent.sid, "sid-1");
        let meta = world.get::<PageMetadata>(stack).expect("meta");
        assert_eq!(meta.icon.favicon_url(), "https://cdn.example/vibe.svg");
    }
}
