use super::{
    AcpModelOption, AgentAttachment, AgentCommand, AgentRunStatus, ApprovalDecision, ClientMessage,
    ServiceMessage,
};
use crate::json::JsonValue;
use crate::room::{ClientOpId, Message, RemoteAgent, RemoteMediaEntry, RemoteSession};
use vmux_macro::VariantNames;

#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, VariantNames)]
pub enum SharedMessage {
    AgentAttach {
        sid: String,
    },
    AgentInput {
        sid: String,
        text: String,
        context: Option<String>,
        attachments: Vec<AgentAttachment>,
        preferred_mode: Option<String>,
    },
    AgentCancel {
        sid: String,
    },
    AgentApprove {
        sid: String,
        call_id: String,
        decision: ApprovalDecision,
    },
    AgentListMedia {
        sid: String,
        query: String,
    },
    ListSessions,
    AgentCommand(SharedAgentCommand),
}

impl From<SharedMessage> for ClientMessage {
    fn from(message: SharedMessage) -> Self {
        Self::Shared(message)
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    VariantNames,
)]
pub enum SharedAgentCommand {
    NewAgentChat {
        client_op_id: ClientOpId,
        prompt: String,
        agent_url: Option<String>,
    },
    ListAgents,
    ListTeam,
    ListModels {
        sid: String,
    },
    SelectModel {
        sid: String,
        model_id: String,
    },
    SetEffort {
        sid: String,
        level: String,
    },
}

impl From<SharedAgentCommand> for AgentCommand {
    fn from(command: SharedAgentCommand) -> Self {
        Self::Shared(command)
    }
}

#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, VariantNames)]
pub enum SharedEvent {
    AgentDelta {
        sid: String,
        text: String,
    },
    AgentRunStatusChanged {
        sid: String,
        status: AgentRunStatus,
    },
    AgentAwaitingApproval {
        sid: String,
        call_id: String,
        name: String,
        args: JsonValue,
    },
    AgentApprovalResolved {
        sid: String,
        call_id: String,
    },
    AgentMessagesSnapshot {
        sid: String,
        messages: Vec<Message>,
    },
    AcpAgentInfo {
        sid: String,
        name: String,
    },
    AcpWorkspaceChanged {
        sid: String,
        name: String,
        branch: String,
        cwd: String,
        workspace_cwd: String,
    },
    AcpModelInfo {
        sid: String,
        config_id: String,
        current_model_id: String,
        models: Vec<AcpModelOption>,
    },
    Session {
        session: RemoteSession,
    },
}

impl From<SharedEvent> for ServiceMessage {
    fn from(event: SharedEvent) -> Self {
        Self::Shared(event)
    }
}

#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum SharedResponse {
    Ok,
    AlreadyApplied,
    Sessions(Vec<RemoteSession>),
    Agents(Vec<RemoteAgent>),
    Media(Vec<RemoteMediaEntry>),
    BrokerJson(String),
    Failed(SharedFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum SharedFailure {
    NotFound,
    Invalid,
    NoDesktop,
    Internal,
}
