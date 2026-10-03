use bevy::prelude::Component;

use vmux_session::RunState;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentRunStateKind {
    Idle,
    Streaming,
    AwaitingApproval,
    Errored,
}

impl From<&RunState> for AgentRunStateKind {
    fn from(state: &RunState) -> Self {
        match state {
            RunState::Idle => AgentRunStateKind::Idle,
            RunState::Streaming => AgentRunStateKind::Streaming,
            RunState::AwaitingApproval { .. } => AgentRunStateKind::AwaitingApproval,
            RunState::Errored(_) => AgentRunStateKind::Errored,
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
        let s = RunState::Idle;
        assert_eq!(AgentRunStateKind::from(&s), AgentRunStateKind::Idle);
    }

    #[test]
    fn from_state_errored() {
        let s = RunState::Errored("oops".into());
        assert_eq!(AgentRunStateKind::from(&s), AgentRunStateKind::Errored);
    }

    #[test]
    fn last_run_state_kind_default_is_idle() {
        assert_eq!(LastRunStateKind::default().0, AgentRunStateKind::Idle);
    }
}
