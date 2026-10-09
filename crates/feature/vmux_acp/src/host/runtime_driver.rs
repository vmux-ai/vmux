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
    pub(super) fn prompt(
        policy: &AcpWorkspacePolicy,
        handoff: Option<String>,
        state: Option<AcpWorkspaceState>,
    ) -> Option<String> {
        let policy = match state {
            Some(AcpWorkspaceState::Bound) | None => None,
            Some(AcpWorkspaceState::Unbound) => Some(policy.unbound.as_str()),
            Some(AcpWorkspaceState::PendingWorktree) => Some(policy.pending_worktree.as_str()),
            Some(AcpWorkspaceState::RepositoryNeedsWorktree) => {
                Some(policy.repository_needs_worktree.as_str())
            }
        };
        match (handoff, policy) {
            (Some(handoff), Some(policy)) => Some(format!("{handoff}\n\n{policy}")),
            (Some(handoff), None) => Some(handoff),
            (None, Some(policy)) => Some(policy.to_string()),
            (None, None) => None,
        }
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
