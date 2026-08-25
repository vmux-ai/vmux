use bevy::prelude::*;
use std::time::Duration;

#[derive(Component, Default)]
#[require(AgentTurnMeta)]
pub enum AgentRunState {
    #[default]
    Idle,
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

impl AgentRunState {
    pub fn status(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Installing { .. } => "installing",
            Self::Streaming => "streaming",
            Self::AwaitingApproval { .. } => "awaiting",
            Self::Errored(_) => "errored",
        }
    }
}

#[derive(Component, Default)]
pub struct AgentTurnMeta {
    pub durations: Vec<u32>,
    pub turn_start: Option<Duration>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_idle() {
        assert!(matches!(AgentRunState::default(), AgentRunState::Idle));
    }

    #[test]
    fn errored_holds_message() {
        let s = AgentRunState::Errored("oops".into());
        match s {
            AgentRunState::Errored(m) => assert_eq!(m, "oops"),
            _ => panic!("wrong variant"),
        }
    }
}
