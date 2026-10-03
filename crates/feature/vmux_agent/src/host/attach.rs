use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_ecs::agent::SessionId;
use vmux_ecs::{AgentWorkingDir, EntityTarget, PageMetadata};

pub(super) use super::attach_driver::AcpAgentAttachment;
use super::attach_driver::{AcpAgentId, AcpAttachmentIcon, AcpResume, PendingAcpAgentAttachment};

pub(super) fn add(app: &mut App) {
    app.add_systems(Update, attach);
}

fn attach(
    attachments: Query<
        (
            Entity,
            &SessionTarget,
            &StackTarget,
            &SessionName,
            &AgentName,
            &AcpId,
            &SessionId,
            &WorkingDirectory,
            &AcpAttachmentIcon,
            &AcpResume,
            Option<&EntityTarget<vmux_chat::host::ChatView>>,
        ),
        Added<PendingAcpAgentAttachment>,
    >,
    mut commands: Commands,
) {
    for (
        operation,
        SessionTarget(session_entity),
        StackTarget(stack),
        SessionName(session_name),
        AgentName(agent_name),
        AcpId(agent_id),
        SessionId(sid),
        WorkingDirectory(cwd),
        AcpAttachmentIcon(icon),
        AcpResume(resume),
        webview,
    ) in &attachments
    {
        let agent_id = agent_id.as_str();
        let name = name.as_str();
        let url = match resume.as_deref() {
            Some(acp_sid) => format!("{}{agent_id}/{acp_sid}", vmux_api::VmuxRoute::SESSIONS_ROOT),
            None => format!("{}{agent_id}", vmux_api::VmuxRoute::SESSIONS_ROOT),
        };
        commands.entity(entity).insert(PageMetadata {
            url: url.clone(),
            title: name.to_string(),
            bg_color: None,
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
        ));
        let anchor = ProcessId::new();
        commands.entity(*session_entity).insert((
            vmux_ecs::agent::AgentSessionRoot,
            vmux_session::AgentId(agent_id.to_string()),
            Cwd(cwd.clone()),
            ProcessAnchor(anchor),
            vmux_session::RunState::default(),
            vmux_session::ApprovalPolicy::default(),
            vmux_session::PromptQueue::default(),
            vmux_ecs::team::Profile::registry(agent_name, agent_id),
            vmux_ecs::team::Agent { sid: sid.clone() },
        ));
        if let Some(resume) = resume.clone() {
            commands
                .entity(*session_entity)
                .insert(vmux_session::AcpSessionId(resume));
        } else {
            commands
                .entity(*session_entity)
                .remove::<vmux_session::AcpSessionId>();
        }
        let view = if let Some(webview) = webview {
            webview.entity()
        } else {
            commands
                .spawn((
                    vmux_layout::Browser::hosted_page_with_icon(&url, name, icon.clone()),
                    vmux_chat::host::ChatView,
                    ChildOf(*stack),
                    anchor,
                ))
                .id()
        };
        commands.entity(view).insert((
            ChildOf(entity),
            PageMetadata {
                url,
                title: session_name.clone(),
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
        commands.entity(operation).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_attach_gives_profile_agent_and_icon() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        add(&mut app);
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(AcpAgentAttachment::new(
            session,
            stack,
            "mistral-vibe",
            "Task",
            "Mistral Vibe",
            "sid-1",
            std::path::PathBuf::from("/tmp"),
            Some("https://cdn.example/vibe.svg".to_string()),
            None,
        ));
        app.update();

        let world = app.world();
        let profile = world
            .get::<vmux_ecs::team::Profile>(session)
            .expect("profile");
        assert_eq!(profile.name, "Mistral Vibe");
        let agent = world.get::<vmux_ecs::team::Agent>(session).expect("agent");
        assert_eq!(agent.sid, "sid-1");
        let meta = world.get::<PageMetadata>(stack).expect("meta");
        assert_eq!(meta.title, "Task");
        assert_eq!(meta.icon.favicon_url(), "https://cdn.example/vibe.svg");
        assert_eq!(
            world
                .get::<EntityTarget<vmux_session::Session>>(stack)
                .unwrap()
                .entity(),
            session
        );
    }
}
