use bevy_ecs::prelude::*;

use crate::protocol::{
    AgentCommand, AgentCommandResult, AgentQuery, AgentQueryResult, AgentRequestId, AgentRunStatus,
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

#[derive(Message)]
pub struct AgentCommandRequest {
    pub request_id: AgentRequestId,
    pub origin: CommandOrigin,
    pub command: AgentCommand,
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
    pub args_json: String,
}

#[derive(Message)]
pub struct AgentCommandResultEvent {
    pub request_id: AgentRequestId,
    pub result: AgentCommandResult,
}

#[derive(Message)]
pub struct AgentQueryResultEvent {
    pub request_id: AgentRequestId,
    pub result: AgentQueryResult,
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
    pub args_json: String,
}

#[derive(Message)]
pub struct PageAgentApprovalResolved {
    pub sid: String,
    pub call_id: String,
}

#[derive(Message)]
pub struct PageAgentSnapshot {
    pub sid: String,
    pub messages_json: String,
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
