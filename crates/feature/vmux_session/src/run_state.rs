use bevy_ecs::prelude::Component;
use std::time::Duration;

#[derive(Component, Default)]
#[require(AgentTurnMeta)]
pub enum RunState {
    Preparing,
    Installing {
        detail: String,
        percent: Option<u8>,
    },
    #[default]
    Idle,
    Streaming,
    AwaitingApproval {
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    Errored(String),
    AuthenticationRequired {
        methods: Vec<vmux_api::protocol::AuthenticationMethod>,
        pending: bool,
    },
}

impl RunState {
    pub fn needs_credentials(&self) -> bool {
        match self {
            Self::AuthenticationRequired { .. } => true,
            Self::Errored(error) => Failure(error).needs_credentials(),
            _ => false,
        }
    }

    pub fn status(&self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Installing { .. } => "installing",
            Self::Idle => "idle",
            Self::Streaming => "streaming",
            Self::AwaitingApproval { .. } => "awaiting",
            Self::Errored(_) => "errored",
            Self::AuthenticationRequired { .. } => "authentication",
        }
    }
}

pub struct Failure<'a>(pub &'a str);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Busy,
    MissingHistory,
    Connection,
    Setup,
    Other,
}

impl Failure<'_> {
    pub fn needs_credentials(&self) -> bool {
        let message = self.0.to_ascii_lowercase();
        message.contains("authentication")
            || message.contains("api key")
            || message.contains("unauthorized")
            || message.contains("401")
    }

    pub fn kind(&self) -> FailureKind {
        if self.in_use() {
            return FailureKind::Busy;
        }
        let message = self.0.to_ascii_lowercase();
        if message.contains("no rollout found") || message.contains("thread not found") {
            return FailureKind::MissingHistory;
        }
        if message.contains("timed out")
            || message.contains("timeout")
            || message.contains("connection refused")
            || message.contains("connection closed")
        {
            return FailureKind::Connection;
        }
        if message.contains("command not found")
            || message.contains("acp installation failed")
            || message.contains("acp setup cancelled")
            || message.contains("authentication")
            || message.contains("api key")
            || message.contains("unauthorized")
            || message.contains("401")
            || message.contains("not installed")
            || message.contains("unsupported version")
            || message.contains("package version")
        {
            return FailureKind::Setup;
        }
        FailureKind::Other
    }

    pub fn in_use(&self) -> bool {
        let message = self.0.to_ascii_lowercase();
        message.contains("thread_active_writer")
            || message.contains("already has an active writer")
            || message.contains("session is in use")
            || message.contains("session_busy")
            || message.contains("session is already open")
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
            ("Session is already open: session-1", true),
            ("Invalid request: session_busy", true),
            ("Invalid request: unknown session", false),
        ] {
            assert_eq!(Failure(message).in_use(), expected, "{message}");
        }
    }

    #[test]
    fn failures_distinguish_recoverable_connections_from_missing_history() {
        for (message, kind) in [
            (
                "session/load failed: no rollout found for thread id abc",
                FailureKind::MissingHistory,
            ),
            ("session/load timed out", FailureKind::Connection),
            ("agent command not found", FailureKind::Setup),
            ("operation failed unexpectedly", FailureKind::Other),
        ] {
            assert!(Failure(message).kind() == kind, "{message}");
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
