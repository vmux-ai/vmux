use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::protocol::AgentRunStatus;

#[cfg(target_arch = "wasm32")]
pub mod page;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteStatus {
    Idle,
    Streaming,
    Interrupted,
    Errored(String),
}

impl From<&AgentRunStatus> for RemoteStatus {
    fn from(status: &AgentRunStatus) -> Self {
        match status {
            AgentRunStatus::Idle => Self::Idle,
            AgentRunStatus::Streaming => Self::Streaming,
            AgentRunStatus::Interrupted => Self::Interrupted,
            AgentRunStatus::Errored(message) => Self::Errored(message.clone()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RemoteApproval {
    pub call_id: String,
    pub name: String,
    pub args_json: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RemoteSession {
    pub sid: String,
    pub name: String,
    pub runtime: String,
    pub model: Option<String>,
    pub cwd: String,
    pub status: RemoteStatus,
    pub approval: Option<RemoteApproval>,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RemoteEvent {
    Session { session: RemoteSession },
    Snapshot { messages: Vec<Message> },
    Delta { text: String },
    Status { status: RemoteStatus },
    Approval { approval: Option<RemoteApproval> },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PairRequest {
    pub token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PromptRequest {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApprovalRequest {
    pub call_id: String,
    pub allow: bool,
}
