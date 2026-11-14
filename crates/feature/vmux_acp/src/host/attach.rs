use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_ecs::{
    EntityTarget, PageMetadata, PageNavigation, PendingPrompt, PendingPromptAttachments,
    ProcessAnchor,
};
use vmux_flex::Visibility;
use vmux_layout::tab::{Tab, TabWorkspace};
use vmux_session::{AgentConversationTitle, PromptQueue, QueuedPrompts, SessionId, TitlePending};

pub(super) use super::attach_driver::AcpAgentAttachment;
use super::attach_driver::{
    AcpAttachmentIcon, AcpId, AcpResume, AgentName, DeferredAttachmentPrompt,
    PendingAcpAgentAttachment, SessionName, SessionTarget, StackTarget, WorkingDirectory,
};
use super::navigation::{InlineTransitionSnapshot, PreparingAgentChatView};
use crate::AcpSessionId;

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, attach.after(vmux_ecs::PageOpenSet::ResolveTarget));
    }
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
            Option<&EntityTarget<vmux_ecs::PageOpenTask>>,
            Option<&EntityTarget<vmux_session::host::SessionView>>,
            Option<&PageNavigation>,
            Option<&DeferredAttachmentPrompt>,
        ),
        Added<PendingAcpAgentAttachment>,
    >,
    titles: Query<(), With<AgentConversationTitle>>,
    transcripts: vmux_session::Transcripts,
    navigations: Query<&PageNavigation>,
    child_of: Query<&ChildOf>,
    children: Query<&Children>,
    mut tabs: Query<&mut Tab>,
    mut workspaces: Query<&mut TabWorkspace>,
    mut queues: QueuedPrompts,
    time: Res<Time>,
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
        open_task,
        webview,
        navigation,
        prompt,
    ) in &attachments
    {
        if navigation.is_some_and(|navigation| {
            !navigations
                .get(*stack)
                .is_ok_and(|current| current == navigation)
        }) {
            commands.entity(operation).despawn();
            continue;
        }
        let agent_id = agent_id.as_str();
        let url = vmux_session::Route::Session(SessionId(sid.clone())).url();
        let page_metadata = PageMetadata {
            url: url.clone(),
            title: session_name.clone(),
            bg_color: None,
            icon: icon.clone(),
        };
        let profile_icon = (!icon.favicon_url().is_empty()).then(|| icon.favicon_url().to_string());
        let mut ancestor = *stack;
        loop {
            if let Ok(mut tab) = tabs.get_mut(ancestor) {
                let cwd = cwd.to_string_lossy().into_owned();
                tab.startup_dir = Some(cwd.clone());
                if let Ok(mut workspace) = workspaces.get_mut(ancestor) {
                    workspace.project_dir.clone_from(&cwd);
                } else {
                    commands
                        .entity(ancestor)
                        .insert(TabWorkspace { project_dir: cwd });
                }
                break;
            }
            let Ok(parent) = child_of.get(ancestor) else {
                break;
            };
            ancestor = parent.parent();
        }
        commands
            .entity(*stack)
            .insert(EntityTarget::<vmux_session::Session>::new(*session_entity));
        if navigation.is_none() {
            commands.entity(*stack).insert(page_metadata.clone());
        }
        let transition_view = webview.map(|webview| webview.entity());
        if transition_view.is_some()
            && let Ok(stack_children) = children.get(*stack)
        {
            for child in stack_children.iter() {
                if Some(child) != transition_view {
                    commands.entity(child).despawn();
                }
            }
        }
        let mut prompt_title_inserted = false;
        if let Some(prompt) = prompt {
            let text = prompt.text.clone().unwrap_or_default();
            if !text.trim().is_empty() || !prompt.attachments.is_empty() {
                if let Some(title) = AgentConversationTitle::from_prompt(&text) {
                    commands
                        .entity(*session_entity)
                        .insert((title, TitlePending));
                    prompt_title_inserted = true;
                }
                queues.enqueue(*session_entity, text, prompt.attachments.clone(), false);
                commands
                    .entity(*stack)
                    .remove::<(PendingPrompt, PendingPromptAttachments)>();
            }
        }
        let anchor = ProcessId::new();
        commands
            .entity(*session_entity)
            .remove::<super::navigation_driver::RetryRequested>();
        commands.entity(*session_entity).insert((
            vmux_ecs::agent::AgentSessionRoot,
            ProcessAnchor(anchor),
            vmux_session::RunState::default(),
            vmux_ecs::team::Profile::registry(agent_name, agent_id).with_icon(profile_icon),
            vmux_ecs::team::Agent { sid: sid.clone() },
        ));
        commands.queue(vmux_session::mutation::Request::local(
            SessionId(sid.clone()),
            vmux_session::mutation::Mutation::BindAgent {
                agent: vmux_session::AgentId(agent_id.to_string()),
                cwd: cwd.clone(),
            },
        ));
        commands
            .entity(*session_entity)
            .entry::<PromptQueue>()
            .or_default();
        if let Some(resume) = resume.clone() {
            let mut session = commands.entity(*session_entity);
            session.insert(AcpSessionId(resume));
            if transcripts.read(*session_entity, |transcript| transcript.messages.is_empty()) {
                session.insert(vmux_session::TranscriptLoading);
            } else {
                session.remove::<vmux_session::TranscriptLoading>();
            }
            if !prompt_title_inserted && !titles.contains(*session_entity) {
                session.insert((AgentConversationTitle(session_name.clone()), TitlePending));
            }
        } else {
            commands
                .entity(*session_entity)
                .remove::<(AcpSessionId, vmux_session::TranscriptLoading)>();
        }
        let view = if let Some(webview) = webview {
            webview.entity()
        } else if let Some(navigation) = navigation {
            let previous = children
                .get(*stack)
                .map(|children| children.iter().collect::<Vec<_>>())
                .unwrap_or_default();
            for surface in &previous {
                commands
                    .entity(*surface)
                    .try_insert(vmux_layout::hosted_page::SurfaceBlocked);
            }
            let handoff = vmux_layout::hosted_page::SurfaceHandoff::new(
                previous,
                navigation.id(),
                0,
                time.elapsed(),
            );
            commands
                .spawn((
                    vmux_layout::Browser::hosted_page_with_icon(&url, session_name, icon.clone()),
                    handoff,
                    vmux_session::host::SessionView,
                    ChildOf(*stack),
                    anchor,
                ))
                .insert(Visibility::Hidden)
                .id()
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
        commands
            .entity(view)
            .insert((page_metadata, anchor, vmux_session::host::SessionView));
        commands.entity(view).remove::<(
            vmux_session::host::SessionManagerView,
            vmux_session::host::SessionCatalogView,
        )>();
        if webview.is_some() {
            commands.entity(view).remove::<(
                PreparingAgentChatView,
                InlineTransitionSnapshot,
                vmux_start::StartInlineTransitionView,
                vmux_ecs::launcher::HostsLauncher,
                vmux_ecs::page::PageReady,
            )>();
        }
        commands
            .entity(*stack)
            .remove::<vmux_start::StartInlineTransition>();
        if let Some(open_task) = open_task
            && let Ok(mut task) = commands.get_entity(open_task.entity())
        {
            task.remove::<vmux_ecs::PageOpenDeferred>()
                .insert(vmux_ecs::PageOpenHandled);
        }
        commands.entity(operation).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource)]
    struct DeferredPrompt(Entity);

    fn queue_prompt(prompt: Res<DeferredPrompt>, mut queues: QueuedPrompts) {
        queues.enqueue(prompt.0, "first".into(), Vec::new(), false);
    }

    #[test]
    fn acp_attach_gives_profile_agent_and_icon() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_session::QueuePlugin));
        app.add_plugins(super::Plugin);
        let session = app.world_mut().spawn(vmux_session::Session).id();
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
        assert_eq!(
            profile.icon.as_deref(),
            Some("https://cdn.example/vibe.svg")
        );
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
        app.add_plugins((MinimalPlugins, vmux_session::QueuePlugin));
        app.add_plugins(super::Plugin);
        app.add_systems(Update, queue_prompt.before(attach));
        let session = app.world_mut().spawn(vmux_session::Session).id();
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

        let mut items = app
            .world_mut()
            .query_filtered::<&PendingPrompt, With<vmux_session::QueuedPrompt>>();
        assert_eq!(items.single(app.world()).unwrap().0, "first");
    }

    #[test]
    fn resumed_attachment_keeps_the_prompt_derived_title() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_session::QueuePlugin));
        app.add_plugins(super::Plugin);
        let session = app.world_mut().spawn(vmux_session::Session).id();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            AcpAgentAttachment::new(
                session,
                stack,
                "codex",
                "Stored title",
                "Codex",
                "sid-1",
                std::path::PathBuf::from("/tmp"),
                None,
                Some("resume-1".into()),
            ),
            DeferredAttachmentPrompt {
                text: Some("Investigate the renderer race".into()),
                attachments: Vec::new(),
            },
        ));

        app.update();

        assert_eq!(
            app.world()
                .get::<AgentConversationTitle>(session)
                .map(|title| title.0.as_str()),
            Some("Investigate the renderer race")
        );
    }

    #[test]
    fn page_open_finishes_only_after_attachment_succeeds() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_session::QueuePlugin));
        app.add_plugins(super::Plugin);
        let session = app.world_mut().spawn(vmux_session::Session).id();
        let stack = app.world_mut().spawn_empty().id();
        let task = app
            .world_mut()
            .spawn((
                vmux_ecs::PageOpenTask {
                    id: vmux_ecs::PageOpenId::new(),
                    stack,
                    url: vmux_session::Route::Session(SessionId("sid-1".into())).url(),
                    request_id: None,
                },
                vmux_ecs::PageOpenDeferred,
            ))
            .id();
        app.world_mut().spawn((
            AcpAgentAttachment::new(
                session,
                stack,
                "codex",
                "Task",
                "Codex",
                "sid-1",
                std::path::PathBuf::from("/tmp"),
                None,
                None,
            ),
            EntityTarget::<vmux_ecs::PageOpenTask>::new(task),
        ));

        app.update();

        assert!(
            app.world()
                .get::<vmux_ecs::PageOpenDeferred>(task)
                .is_none()
        );
        assert!(app.world().get::<vmux_ecs::PageOpenHandled>(task).is_some());
    }

    #[test]
    fn non_inline_attachment_retains_the_previous_surface_until_paint() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_session::QueuePlugin));
        app.add_plugins(super::Plugin);
        let session = app.world_mut().spawn(vmux_session::Session).id();
        let navigation = vmux_ecs::PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(navigation)).id();
        let previous = app
            .world_mut()
            .spawn((vmux_layout::Browser, ChildOf(stack)))
            .id();
        app.world_mut().spawn((
            AcpAgentAttachment::new(
                session,
                stack,
                "codex",
                "Task",
                "Codex",
                "sid-1",
                std::path::PathBuf::from("/tmp"),
                None,
                None,
            ),
            PageNavigation::new(navigation),
        ));

        app.update();

        assert!(app.world().get_entity(previous).is_ok());
        assert!(
            app.world()
                .get::<vmux_layout::hosted_page::SurfaceBlocked>(previous)
                .is_some()
        );
        let incoming = app
            .world()
            .get::<Children>(stack)
            .unwrap()
            .iter()
            .find(|child| *child != previous)
            .unwrap();
        assert!(
            app.world()
                .get::<vmux_layout::hosted_page::SurfaceHandoff>(incoming)
                .is_some()
        );
        let now = app.world().resource::<Time>().elapsed();
        assert!(
            app.world()
                .get::<vmux_layout::hosted_page::SurfaceHandoff>(incoming)
                .is_some_and(|handoff| {
                    handoff.timed_out(now + std::time::Duration::from_secs(10))
                })
        );
        assert_eq!(
            app.world().get::<Visibility>(incoming),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn superseded_page_navigation_discards_pending_attachment() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_session::QueuePlugin));
        app.add_plugins(super::Plugin);
        let session = app.world_mut().spawn(vmux_session::Session).id();
        let current = vmux_ecs::PageOpenId::new();
        let stale = vmux_ecs::PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(current)).id();
        let operation = app
            .world_mut()
            .spawn((
                AcpAgentAttachment::new(
                    session,
                    stack,
                    "codex",
                    "Task",
                    "Codex",
                    "sid-1",
                    std::path::PathBuf::from("/tmp"),
                    None,
                    None,
                ),
                PageNavigation::new(stale),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(operation).is_err());
        assert!(app.world().get::<PageMetadata>(stack).is_none());
        assert!(app.world().get::<ProcessAnchor>(session).is_none());
    }
}
