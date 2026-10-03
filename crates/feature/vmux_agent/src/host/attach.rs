use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_ecs::{Cwd, EntityTarget, PageIcon, PageMetadata, ProcessAnchor};
use vmux_session::{PromptQueue, SessionId};

pub(super) struct AttachPlugin;

impl Plugin for AttachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, attach);
    }
}

#[derive(Bundle)]
pub(super) struct AcpAgentAttachment {
    pending: PendingAcpAgentAttachment,
    operation_name: Name,
    session: SessionTarget,
    stack: StackTarget,
    session_name: SessionName,
    agent_name: AgentName,
    agent_id: AcpId,
    session_id: SessionId,
    working_directory: WorkingDirectory,
    icon: AcpAttachmentIcon,
    resume: AcpResume,
}

impl AcpAgentAttachment {
    pub(super) fn new(
        session: Entity,
        stack: Entity,
        agent_id: impl Into<String>,
        session_name: impl Into<String>,
        agent_name: impl Into<String>,
        sid: impl Into<String>,
        cwd: impl Into<std::path::PathBuf>,
        icon: Option<String>,
        resume: Option<String>,
    ) -> Self {
        Self {
            pending: PendingAcpAgentAttachment,
            operation_name: Name::new("Attach ACP agent"),
            session: SessionTarget(session),
            stack: StackTarget(stack),
            session_name: SessionName(session_name.into()),
            agent_name: AgentName(agent_name.into()),
            agent_id: AcpId(agent_id.into()),
            session_id: SessionId(sid.into()),
            working_directory: WorkingDirectory(cwd.into()),
            icon: AcpAttachmentIcon(PageIcon::favicon(icon.unwrap_or_default())),
            resume: AcpResume(resume),
        }
    }
}

#[derive(Component)]
struct PendingAcpAgentAttachment;

#[derive(Component)]
struct SessionTarget(Entity);

#[derive(Component)]
struct StackTarget(Entity);

#[derive(Component)]
struct SessionName(String);

#[derive(Component)]
struct AgentName(String);

#[derive(Component)]
struct WorkingDirectory(std::path::PathBuf);

#[derive(Component)]
struct AcpId(String);

#[derive(Component)]
struct AcpAttachmentIcon(PageIcon);

#[derive(Component)]
struct AcpResume(Option<String>);

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
        let url = vmux_session::Route::Session(vmux_session::SessionId(sid.clone())).url();
        commands.entity(*stack).insert((
            EntityTarget::<vmux_session::Session>::new(*session_entity),
            PageMetadata {
                url: url.clone(),
                title: session_name.clone(),
                bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
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
                    vmux_layout::Browser::native_page(&url, session_name),
                    vmux_chat::host::ChatView,
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
            vmux_chat::host::ChatView,
        ));
        commands
            .entity(view)
            .remove::<vmux_chat::host::SessionManagerView>();
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
        app.add_plugins((MinimalPlugins, AttachPlugin));
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
        app.add_plugins((MinimalPlugins, AttachPlugin));
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
