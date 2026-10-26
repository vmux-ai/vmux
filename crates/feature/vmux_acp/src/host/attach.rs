use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_ecs::{Cwd, EntityTarget, PageMetadata, ProcessAnchor};
use vmux_session::{AgentConversationTitle, Id, PromptQueue, Session, TitlePending};

pub(super) use super::attach_driver::AcpAgentAttachment;
use super::attach_driver::{
    AcpAttachmentIcon, AcpId, AcpResume, AgentName, PendingAcpAgentAttachment, SessionName,
    SessionTarget, StackTarget, WorkingDirectory,
};
use crate::session::{AcpSessionLink, AcpSessionLinks};

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
            &Id<Session>,
            &WorkingDirectory,
            &AcpAttachmentIcon,
            &AcpResume,
            Option<&EntityTarget<vmux_session::host::SessionView>>,
        ),
        Added<PendingAcpAgentAttachment>,
    >,
    titles: Query<(), With<AgentConversationTitle>>,
    transcripts: vmux_session::Transcripts,
    links: AcpSessionLinks,
    mut commands: Commands,
) {
    for (
        operation,
        SessionTarget(session_entity),
        StackTarget(stack),
        SessionName(session_name),
        AgentName(agent_name),
        AcpId(agent_id),
        session_id,
        WorkingDirectory(cwd),
        AcpAttachmentIcon(icon),
        AcpResume(resume),
        webview,
    ) in &attachments
    {
        let agent_id = agent_id.as_str();
        let sid = session_id.as_str();
        let url = vmux_session::Route::Session(session_id.clone()).url();
        let profile_icon = (!icon.favicon_url().is_empty()).then(|| icon.favicon_url().to_string());
        commands.entity(*stack).insert((
            EntityTarget::<Session>::new(*session_entity),
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
            vmux_ecs::team::Agent {
                sid: sid.to_string(),
            },
        ));
        commands.spawn(
            vmux_ecs::team::Profile::registry(agent_name, agent_id)
                .with_icon(profile_icon)
                .owned_by(*session_entity),
        );
        commands
            .entity(*session_entity)
            .entry::<PromptQueue>()
            .or_default();
        if let Some(resume) = resume.clone() {
            let mut session = commands.entity(*session_entity);
            if transcripts.get(*session_entity).messages.is_empty() {
                session.insert(vmux_session::TranscriptLoading);
            } else {
                session.remove::<vmux_session::TranscriptLoading>();
            }
            if !titles.contains(*session_entity) {
                session.insert((AgentConversationTitle(session_name.clone()), TitlePending));
            }
            links.set(
                &mut commands,
                *session_entity,
                &AcpSessionLink::kind(agent_id),
                &resume,
                None,
            );
        } else {
            links.remove(&mut commands, *session_entity);
            commands
                .entity(*session_entity)
                .remove::<vmux_session::TranscriptLoading>();
        }
        let view = if let Some(webview) = webview {
            webview.entity()
        } else {
            commands
                .spawn((
                    vmux_layout::Browser::hosted_page_with_icon(&url, session_name, icon.clone()),
                    vmux_session::host::SessionView,
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
            vmux_session::host::SessionView,
        ));
        commands.entity(view).remove::<(
            vmux_session::host::SessionManagerView,
            vmux_session::host::SessionCatalogView,
        )>();
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
    use bevy::ecs::system::RunSystemOnce;

    #[derive(Resource)]
    struct DeferredPrompt(Entity);

    fn queue_prompt(prompt: Res<DeferredPrompt>, mut commands: Commands) {
        let mut queue = PromptQueue::default();
        queue.enqueue("first".into());
        commands.entity(prompt.0).insert(queue);
    }

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

        let (profile_name, profile_icon) = app
            .world_mut()
            .run_system_once(move |profiles: vmux_ecs::team::Profiles| {
                profiles.get(session).map(|profile| {
                    (
                        profile.name().to_string(),
                        profile.icon().map(str::to_string),
                    )
                })
            })
            .unwrap()
            .expect("profile");
        assert_eq!(profile_name, "Mistral Vibe");
        assert_eq!(
            profile_icon.as_deref(),
            Some("https://cdn.example/vibe.svg")
        );
        let world = app.world();
        let agent = world.get::<vmux_ecs::team::Agent>(session).expect("agent");
        assert_eq!(agent.sid, "sid-1");
        let meta = world.get::<PageMetadata>(stack).expect("meta");
        assert_eq!(meta.title, "Task");
        assert_eq!(meta.icon.favicon_url(), "https://cdn.example/vibe.svg");
        assert_eq!(
            world.get::<EntityTarget<Session>>(stack).unwrap().entity(),
            session
        );
    }

    #[test]
    fn acp_attach_preserves_queued_prompts() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        add(&mut app);
        app.add_systems(Update, queue_prompt.before(attach));
        let session = app.world_mut().spawn_empty().id();
        app.insert_resource(DeferredPrompt(session));
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
