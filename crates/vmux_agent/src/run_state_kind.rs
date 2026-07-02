use bevy::prelude::Component;

use crate::run_state::AgentRunState;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentRunStateKind {
    Idle,
    Streaming,
    RunningTool,
    AwaitingApproval,
    Errored,
}

impl From<&AgentRunState> for AgentRunStateKind {
    fn from(state: &AgentRunState) -> Self {
        match state {
            AgentRunState::Idle => AgentRunStateKind::Idle,
            AgentRunState::Installing { .. } => AgentRunStateKind::Idle,
            AgentRunState::Streaming => AgentRunStateKind::Streaming,
            AgentRunState::AwaitingApproval { .. } => AgentRunStateKind::AwaitingApproval,
            AgentRunState::Errored(_) => AgentRunStateKind::Errored,
        }
    }
}

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LastRunStateKind(pub AgentRunStateKind);

impl Default for LastRunStateKind {
    fn default() -> Self {
        Self(AgentRunStateKind::Idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_state_idle() {
        let s = AgentRunState::Idle;
        assert_eq!(AgentRunStateKind::from(&s), AgentRunStateKind::Idle);
    }

    #[test]
    fn from_state_errored() {
        let s = AgentRunState::Errored("oops".into());
        assert_eq!(AgentRunStateKind::from(&s), AgentRunStateKind::Errored);
    }

    #[test]
    fn last_run_state_kind_default_is_idle() {
        assert_eq!(LastRunStateKind::default().0, AgentRunStateKind::Idle);
    }
}
