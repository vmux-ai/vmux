use bevy::prelude::*;
use vmux_api::PageIcon;
use vmux_session::SessionId;

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
pub(super) struct PendingAcpAgentAttachment;

#[derive(Component)]
pub(super) struct SessionTarget(pub(super) Entity);

#[derive(Component)]
pub(super) struct StackTarget(pub(super) Entity);

#[derive(Component)]
pub(super) struct SessionName(pub(super) String);

#[derive(Component)]
pub(super) struct AgentName(pub(super) String);

#[derive(Component)]
pub(super) struct WorkingDirectory(pub(super) std::path::PathBuf);

#[derive(Component)]
pub(super) struct AcpId(pub(super) String);

#[derive(Component)]
pub(super) struct AcpAttachmentIcon(pub(super) PageIcon);

#[derive(Component)]
pub(super) struct AcpResume(pub(super) Option<String>);
