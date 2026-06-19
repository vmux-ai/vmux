use bevy::prelude::*;

use crate::protocol::{AgentCommand, AgentQuery, AgentRequestId, AgentRunStatus};

#[derive(Message)]
pub struct AgentCommandRequest {
    pub request_id: AgentRequestId,
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
    pub name: String,
    pub args_json: String,
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
pub struct PageAgentSnapshot {
    pub sid: String,
    pub messages_json: String,
}
