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
#[path = "run_state_kind.test.rs"]
mod tests;
