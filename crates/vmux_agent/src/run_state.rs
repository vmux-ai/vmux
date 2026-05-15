use bevy::prelude::*;
use bevy::tasks::Task;
use crossbeam_channel::Receiver;

use crate::stream::{PartialToolUse, StreamEvent};

#[derive(Component, Default)]
pub enum AgentRunState {
    #[default]
    Idle,
    Streaming {
        rx: Receiver<StreamEvent>,
        _task: Task<()>,
        partial: Option<PartialToolUse>,
    },
    RunningTool {
        call_id: String,
        task: Task<ToolDispatchOutput>,
    },
    AwaitingApproval {
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    Errored(String),
}

#[derive(Clone, Debug)]
pub struct ToolDispatchOutput {
    pub call_id: String,
    pub content: String,
    pub is_error: bool,
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
