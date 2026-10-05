use bevy::prelude::*;
use vmux_api::PageIcon;
use vmux_ecs::AgentWorkingDir;
use vmux_ecs::agent::SessionId;

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
        Self {
            pending: PendingAcpAgentAttachment,
            name: Name::new(name.into()),
            agent_id: AcpAgentId(agent_id.into()),
            session_id: SessionId(sid.into()),
            working_directory: AgentWorkingDir(cwd.into()),
            icon: AcpAttachmentIcon(PageIcon::favicon(icon.unwrap_or_default())),
            resume: AcpResume(resume),
        }
    }
}

#[derive(Component)]
pub(super) struct PendingAcpAgentAttachment;

#[derive(Component)]
pub(super) struct AcpAgentId(pub(super) String);

#[derive(Component)]
pub(super) struct AcpAttachmentIcon(pub(super) PageIcon);

#[derive(Component)]
pub(super) struct AcpResume(pub(super) Option<String>);
