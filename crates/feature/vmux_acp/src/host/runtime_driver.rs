use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::EntityTarget;
use vmux_layout::tab::{Tab, TabWorkspace};
use vmux_session::Session;

use crate::policy::AcpWorkspacePolicy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AcpWorkspaceState {
    Bound,
    Unbound,
    PendingWorktree,
    RepositoryNeedsWorktree,
}

#[derive(SystemParam)]
pub(super) struct PromptWorkspace<'w, 's> {
    child_of: Query<'w, 's, &'static ChildOf>,
    session_views: Query<'w, 's, (Entity, &'static EntityTarget<Session>)>,
    tabs: Query<'w, 's, &'static Tab>,
    workspaces: Query<'w, 's, (), With<TabWorkspace>>,
    pending: Query<'w, 's, (), With<vmux_space::PendingProject>>,
    worktrees: Query<'w, 's, (), With<vmux_space::RepositoryNeedsWorktree>>,
}

impl PromptWorkspace<'_, '_> {
    const TITLE_INSTRUCTION: &'static str = "Before answering, call set_conversation_title with a concise 3 to 7 word model-written title and a one-sentence summary of the user's goal and current outcome. Do not copy the user's prompt verbatim.";

    pub(super) fn prompt(
        policy: &AcpWorkspacePolicy,
        handoff: Option<String>,
        state: Option<AcpWorkspaceState>,
        title_pending: bool,
    ) -> Option<String> {
        let policy = match state {
            Some(AcpWorkspaceState::Bound) | None => None,
            Some(AcpWorkspaceState::Unbound) => Some(policy.unbound.as_str()),
            Some(AcpWorkspaceState::PendingWorktree) => Some(policy.pending_worktree.as_str()),
            Some(AcpWorkspaceState::RepositoryNeedsWorktree) => {
                Some(policy.repository_needs_worktree.as_str())
            }
        };
        let mut context = Vec::new();
        if title_pending {
            context.push(Self::TITLE_INSTRUCTION.to_string());
        }
        if let Some(handoff) = handoff {
            context.push(handoff);
        }
        if let Some(policy) = policy {
            context.push(policy.to_string());
        }
        (!context.is_empty()).then(|| context.join("\n\n"))
    }

    pub(super) fn state(&self, entity: Entity) -> Option<AcpWorkspaceState> {
        let mut current = self
            .session_views
            .iter()
            .find(|(_, target)| target.entity() == entity)
            .map(|(stack, _)| stack)?;
        loop {
            if let Ok(tab) = self.tabs.get(current) {
                let state = match tab.startup_dir.as_deref() {
                    Some(_) if self.worktrees.contains(current) => {
                        AcpWorkspaceState::RepositoryNeedsWorktree
                    }
                    Some(_) => AcpWorkspaceState::Bound,
                    None if self.workspaces.contains(current) => AcpWorkspaceState::Bound,
                    None if self.pending.contains(current) => AcpWorkspaceState::PendingWorktree,
                    None => AcpWorkspaceState::Unbound,
                };
                return Some(state);
            }
            current = self.child_of.get(current).ok()?.parent();
        }
    }
}
