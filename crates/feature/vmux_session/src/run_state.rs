use bevy_ecs::prelude::Component;
use std::time::Duration;

#[derive(Component, Default)]
#[require(AgentTurnMeta)]
pub enum RunState {
    #[default]
    Idle,
    Streaming,
    AwaitingApproval {
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    Errored(String),
}

impl RunState {
    pub fn status(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Streaming => "streaming",
            Self::AwaitingApproval { .. } => "awaiting",
            Self::Errored(_) => "errored",
        }
    }
}

pub struct Failure<'a>(pub &'a str);

impl Failure<'_> {
    pub fn in_use(&self) -> bool {
        self.0.contains("thread_active_writer")
            || self.0.contains("already has an active writer")
            || self.0.contains("session is in use by another Codex client")
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
    fn active_writer_refusals_are_distinct_from_other_failures() {
        for (message, expected) in [
            (
                "Invalid request: {\"reason\":\"thread_active_writer\"}",
                true,
            ),
            ("thread abc already has an active writer", true),
            ("This Codex session is in use by another Codex client", true),
            ("session/load timed out", false),
            ("Invalid request: unknown session", false),
        ] {
            assert_eq!(Failure(message).in_use(), expected, "{message}");
        }
    }

    #[test]
    fn default_is_idle() {
        assert!(matches!(RunState::default(), RunState::Idle));
    }

    #[test]
    fn errored_holds_message() {
        let s = RunState::Errored("oops".into());
        match s {
            RunState::Errored(m) => assert_eq!(m, "oops"),
            _ => panic!("wrong variant"),
        }
    }
}
