use bevy_ecs::prelude::*;

use crate::protocol::{
    AgentCommand, AgentCommandResult, AgentQuery, AgentRequestId, AgentRunStatus, JsonValue,
    ProcessId,
};

#[derive(Clone, Debug, Default)]
pub enum CommandOrigin {
    #[default]
    User,
    Agent {
        sid: Option<String>,
        anchor: Option<ProcessId>,
    },
}

impl CommandOrigin {
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent { .. })
    }

    pub fn allows_focus(&self, requested: bool) -> bool {
        requested && !self.is_agent()
    }
}

#[derive(Message)]
pub struct AgentCommandRequest {
    pub request_id: AgentRequestId,
    pub origin: CommandOrigin,
    pub command: AgentCommand,
}

impl AgentCommandRequest {
    pub fn response(&self, result: AgentCommandResult) -> crate::protocol::ClientMessage {
        crate::protocol::ClientMessage::AgentCommandResponse {
            request_id: self.request_id,
            result,
        }
    }
}

#[derive(Message)]
pub struct AgentQueryRequest {
    pub request_id: AgentRequestId,
    pub query: AgentQuery,
}

#[derive(Message)]
pub struct AgentToolCallRequest {
    pub request_id: AgentRequestId,
    pub sid: String,
    pub name: String,
    pub args: JsonValue,
}

#[derive(Message)]
pub struct PageAgentDelta {
    pub sid: String,
    pub text: String,
}

#[derive(Message)]
pub struct PageAgentRunStatus {
    pub sid: String,
    pub status: AgentRunStatus,
}

#[derive(Message)]
pub struct PageAgentAwaitingApproval {
    pub sid: String,
    pub call_id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Message)]
pub struct PageAgentApprovalResolved {
    pub sid: String,
    pub call_id: String,
}

#[derive(Message)]
pub struct PageAgentSnapshot {
    pub sid: String,
    pub messages: Vec<crate::message::Message>,
}

#[derive(Message)]
pub struct PageAgentInfo {
    pub sid: String,
    pub name: String,
}

#[derive(Message)]
pub struct PageAgentWorkspaceChanged {
    pub sid: String,
    pub name: String,
    pub branch: String,
    pub cwd: String,
    pub workspace_cwd: String,
}

#[derive(Message)]
pub struct PageAgentModelInfo {
    pub sid: String,
    pub config_id: String,
    pub current_model_id: String,
    pub models: Vec<crate::protocol::AcpModelOption>,
}

#[derive(Message)]
pub struct PageAgentModelSelectionResult {
    pub sid: String,
    pub request_id: u64,
    pub model_id: String,
    pub succeeded: bool,
}

#[derive(Message)]
pub struct PageAgentModeInfo {
    pub sid: String,
    pub config_id: String,
    pub current_mode_id: String,
    pub modes: Vec<crate::protocol::AcpModeOption>,
}

#[derive(Message)]
pub struct PageAgentModeSelectionResult {
    pub sid: String,
    pub request_id: u64,
    pub mode_id: String,
    pub succeeded: bool,
}

#[derive(Message)]
pub struct PageAgentSessionCreated {
    pub sid: String,
    pub acp_session_id: String,
}

#[derive(Message)]
pub struct PageAgentAcpTerminalCreated {
    pub sid: String,
    pub terminal_id: String,
    pub process_id: ProcessId,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}
