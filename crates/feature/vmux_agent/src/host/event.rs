use bevy::prelude::*;
use vmux_api::ProcessId;
use vmux_api::protocol::{AcpSessionConfig, AgentRequestId, AgentRunStatus, JsonValue};

pub use vmux_api::protocol::ApprovalDecision;
pub use vmux_ecs::agent::{AgentRequestInput, CommandOrigin};

#[vmux_api::service_message(AgentToolCall)]
pub struct AgentToolCallRequest {
    pub request_id: AgentRequestId,
    pub sid: String,
    pub name: String,
    pub args: JsonValue,
}

#[vmux_api::service_message(SharedEvent::AgentDelta)]
pub struct UiAgentDelta {
    pub sid: String,
    pub text: String,
}

#[vmux_api::service_message(SharedEvent::AgentRunStatusChanged)]
pub struct UiAgentRunStatus {
    pub sid: String,
    pub status: AgentRunStatus,
}

#[vmux_api::service_message(SharedEvent::AgentApprovalResolved)]
pub struct UiAgentApprovalResolved {
    pub sid: String,
    pub call_id: String,
}

#[vmux_api::service_message(SharedEvent::AgentMessagesSnapshot)]
pub struct UiAgentSnapshot {
    pub sid: String,
    pub messages: Vec<vmux_api::room::Message>,
}

#[vmux_api::service_message(SharedEvent::AcpAgentInfo)]
pub struct UiAgentInfo {
    pub sid: String,
    pub name: String,
}

#[vmux_api::service_message(SharedEvent::AcpWorkspaceChanged)]
pub struct UiAgentWorkspaceChanged {
    pub sid: String,
    pub branch: String,
    pub cwd: String,
    pub workspace_cwd: String,
}

#[vmux_api::service_message(AcpSessionConfigState)]
pub struct UiAgentSessionConfigState {
    pub sid: String,
    pub configs: Vec<AcpSessionConfig>,
}

#[vmux_api::service_message(AcpSessionConfigSelectionResult)]
pub struct UiAgentSessionConfigSelectionResult {
    pub sid: String,
    pub request_id: u64,
    pub config_id: Option<String>,
    pub value: String,
    pub succeeded: bool,
}

#[vmux_api::service_message(AcpSessionCreated)]
pub struct UiAgentSessionCreated {
    pub sid: String,
    pub acp_session_id: String,
}

#[vmux_api::service_message(AcpTerminalCreated)]
pub struct UiAgentAcpTerminalCreated {
    pub sid: String,
    pub process_id: ProcessId,
}

#[derive(Event, Clone, Debug)]
pub struct AgentApprovalRequest {
    pub session: Entity,
    pub call_id: String,
    pub name: String,
}

#[derive(Event, Clone, Debug)]
pub struct AgentApprovalReply {
    pub session: Entity,
    pub call_id: String,
    pub decision: ApprovalDecision,
}
