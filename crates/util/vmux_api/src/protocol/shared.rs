use super::{AgentAttachment, AgentRunStatus, ApprovalDecision, ClientMessage, ServiceMessage};
use crate::conversation::{ClientOpId, Message, RemoteAgent, RemoteMediaEntry, RemoteSession};
use crate::json::JsonValue;
#[vmux_macro::variant_names]
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
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
    AgentNewChat {
        client_op_id: ClientOpId,
        prompt: String,
        agent_url: Option<String>,
    },
    AgentListAgents,
    AgentListTeam,
    AgentListModels {
        sid: String,
    },
    AgentSelectModel {
        sid: String,
        model_id: String,
    },
    AgentSetEffort {
        sid: String,
        level: String,
    },
}

impl From<SharedMessage> for ClientMessage {
    fn from(message: SharedMessage) -> Self {
        Self::Shared(message)
    }
}

#[vmux_macro::variant_names]
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
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
