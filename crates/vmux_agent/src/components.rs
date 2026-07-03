use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::{AgentKind, AgentVariant};

#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AgentSession {
    pub kind: AgentKind,
    pub variant: AgentVariant,
    pub sid: String,
    pub provider: String,
    pub model: String,
}

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentMessages(pub Vec<Message>);

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AgentApprovalPolicy {
    pub auto: HashSet<String>,
}

/// FIFO of prompts waiting to be dispatched to this session's agent. Drained one at a time
/// while the session is idle; `paused` holds the queue after an interrupt until the user
/// resumes, clears, or submits again.
#[derive(Component, Clone, Debug, Default)]
pub struct PromptQueue {
    pub items: VecDeque<String>,
    pub paused: bool,
}

impl PromptQueue {
    /// The gate for dispatching the next prompt: idle, not paused, and something queued.
    pub fn ready(&self, idle: bool) -> bool {
        idle && !self.paused && !self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_components_default_constructible() {
        let _ = AgentMessages::default();
        let _ = AgentApprovalPolicy::default();
        let _ = PromptQueue::default();
    }

    #[test]
    fn prompt_queue_ready_gate() {
        let mut q = PromptQueue::default();
        assert!(!q.ready(true)); // empty
        q.items.push_back("a".into());
        assert!(q.ready(true)); // idle + queued
        assert!(!q.ready(false)); // busy
        q.paused = true;
        assert!(!q.ready(true)); // paused
    }
}
