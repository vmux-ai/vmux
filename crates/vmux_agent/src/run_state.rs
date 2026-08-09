use bevy::prelude::*;
use std::time::Duration;

#[derive(Component, Default)]
#[require(AgentTurnMeta)]
pub enum AgentRunState {
    #[default]
    Idle,
    /// Downloading/installing the agent's runtime or package before first spawn.
    Installing {
        pct: Option<u8>,
        message: String,
    },
    Streaming,
    AwaitingApproval {
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    Errored(String),
}

/// Per-session record of finished turn wall-clock, for the chat page's resting
/// `Worked for Ns` header. Runtime-only. `turn_start` is `Time::elapsed()` at the current
/// turn's first `Streaming`; each `Streaming → Idle/Errored` pushes one entry to `durations`.
#[derive(Component, Default)]
pub struct AgentTurnMeta {
    pub durations: Vec<u32>,
    pub turn_start: Option<Duration>,
}

#[cfg(test)]
#[path = "run_state.test.rs"]
mod tests;
