use bevy::prelude::*;

use crate::protocol::{
    AgentCommand, AgentCommandResult, AgentQuery, AgentQueryResult, AgentRequestId, AgentRunStatus,
    ProcessId,
};

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

/// An `AgentCommandResult` the service routed back to this client (correlated by
/// `request_id`). Surfaced as a Bevy message so in-process issuers — like the
/// le-chat host-MCP bridge — can await their own command results. The normal
/// in-app command handlers do NOT consume this; they *produce* the underlying
/// response that the service correlates.
#[derive(Message)]
pub struct AgentCommandResultEvent {
    pub request_id: AgentRequestId,
    pub result: AgentCommandResult,
}

/// An `AgentQueryResult` the service routed back to this client (correlated by
/// `request_id`). See [`AgentCommandResultEvent`].
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
pub struct PageAgentSnapshot {
    pub sid: String,
    pub messages_json: String,
}
