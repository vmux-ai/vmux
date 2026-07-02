use bevy::prelude::*;

#[derive(Component, Default)]
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
