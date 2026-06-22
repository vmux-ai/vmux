use bevy::prelude::*;

use crate::protocol::{AgentCommand, AgentQuery, AgentRequestId, AgentRunStatus, ProcessId};

/// Who issued a command relayed through the agent request plumbing. Most
/// `AgentCommandRequest`s originate from an agent (`Agent`), but some user
/// actions (e.g. opening a history entry) reuse the same path. A CLI agent is
/// identified by its MCP `anchor` (`ProcessId`); a page agent by its `sid`.
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
