use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_ecs::{Cwd, EntityTarget, PageMetadata, ProcessAnchor};
use vmux_session::{PromptQueue, SessionId};

pub(super) use super::attach_driver::AcpAgentAttachment;
use super::attach_driver::{
    AcpAttachmentIcon, AcpId, AcpResume, AgentName, PendingAcpAgentAttachment, SessionName,
    SessionTarget, StackTarget, WorkingDirectory,
};
use crate::AcpSessionId;

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
            Option<&EntityTarget<vmux_session::host::ChatView>>,
        ),
        Added<PendingAcpAgentAttachment>,
    >,
    queues: Query<(), With<PromptQueue>>,
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
        let url = vmux_session::Route::Session(SessionId(sid.clone())).url();
        commands.entity(*stack).insert((
            EntityTarget::<vmux_session::Session>::new(*session_entity),
            PageMetadata {
                url: url.clone(),
                title: session_name.clone(),
                bg_color: None,
                icon: icon.clone(),
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
            vmux_ecs::team::Profile::registry(agent_name, agent_id),
            vmux_ecs::team::Agent { sid: sid.clone() },
        ));
        if !queues.contains(*session_entity) {
            commands
                .entity(*session_entity)
                .insert(PromptQueue::default());
        }
        if let Some(resume) = resume.clone() {
            commands
                .entity(*session_entity)
                .insert(AcpSessionId(resume));
        } else {
            commands.entity(*session_entity).remove::<AcpSessionId>();
        }
        let view = if let Some(webview) = webview {
            webview.entity()
        } else {
            commands
                .spawn((
                    vmux_layout::Browser::hosted_page_with_icon(&url, session_name, icon.clone()),
                    vmux_session::host::ChatView,
                    ChildOf(*stack),
                    anchor,
                ))
                .id()
        };
        commands.entity(view).insert((
            PageMetadata {
                url,
                title: session_name.clone(),
                bg_color: None,
                icon: icon.clone(),
            },
            anchor,
            vmux_session::host::ChatView,
        ));
        commands
            .entity(view)
            .remove::<vmux_session::host::SessionManagerView>();
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
        let session = app.world_mut().spawn_empty().id();
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

    #[test]
    fn acp_attach_preserves_queued_prompts() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        add(&mut app);
        let mut queue = PromptQueue::default();
        queue.enqueue("first".into());
        let session = app.world_mut().spawn(queue).id();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(AcpAgentAttachment::new(
            session,
            stack,
            "codex",
            "Task",
            "Codex",
            "sid-1",
            std::path::PathBuf::from("/tmp"),
            None,
            None,
        ));

        app.update();

        let queue = app.world().get::<PromptQueue>(session).unwrap();
        assert_eq!(queue.items.len(), 1);
        assert_eq!(queue.items.front().unwrap().text, "first");
    }
}
