use bevy::prelude::*;
use vmux_api::PageIcon;
use vmux_api::protocol::AgentAttachment;
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

impl AcpResume {
    pub(super) fn for_agent(
        agent_id: &str,
        fallback: Option<String>,
        has_history: bool,
        provider: Option<(
            &vmux_session::ProviderKind,
            &vmux_session::ProviderSessionId,
        )>,
    ) -> Self {
        if has_history && fallback.is_some() {
            return Self(fallback);
        }
        if let Some((kind, sid)) = provider
            && kind.0 == format!("acp:{agent_id}")
        {
            return Self(Some(sid.0.clone()));
        }
        Self(fallback)
    }
}

#[derive(Component)]
pub(super) struct DeferredAttachmentPrompt {
    pub(super) text: Option<String>,
    pub(super) attachments: Vec<AgentAttachment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumes_the_catalog_owner_using_its_original_conversation_id() {
        let sid = vmux_session::ProviderSessionId("original-conversation".into());
        for (owner, has_history, expected) in [
            ("acp:codex-acp", false, "original-conversation"),
            ("acp:codex-acp", true, "attached-conversation"),
            ("acp:claude-acp", false, "attached-conversation"),
            ("codex", false, "attached-conversation"),
        ] {
            let kind = vmux_session::ProviderKind(owner.into());
            let resume = AcpResume::for_agent(
                "codex-acp",
                Some("attached-conversation".into()),
                has_history,
                Some((&kind, &sid)),
            );
            assert_eq!(resume.0.as_deref(), Some(expected));
        }
    }
}
