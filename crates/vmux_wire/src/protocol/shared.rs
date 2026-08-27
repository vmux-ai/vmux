use super::{
    AcpModelOption, AgentAttachment, AgentCommand, AgentRunStatus, ApprovalDecision, ClientMessage,
    ServiceMessage,
};
use crate::room::{ClientOpId, RemoteAgent, RemoteMediaEntry, RemoteSession};
use vmux_macro::VariantNames;

#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, VariantNames)]
pub enum SharedMessage {
    Agent { sid: String, action: AgentAction },
    ListSessions,
    AgentCommand(SharedAgentCommand),
}

#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, VariantNames)]
pub enum AgentAction {
    Attach,
    Input {
        text: String,
        context: Option<String>,
        attachments: Vec<AgentAttachment>,
    },
    Cancel,
    Approve {
        call_id: String,
        decision: ApprovalDecision,
    },
    ListMedia {
        query: String,
    },
}

impl SharedMessage {
    pub fn agent(sid: impl Into<String>, action: AgentAction) -> Self {
        Self::Agent {
            sid: sid.into(),
            action,
        }
    }
}

impl From<SharedMessage> for ClientMessage {
    fn from(message: SharedMessage) -> Self {
        Self::Shared(message)
    }
}

#[derive(
    Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, VariantNames,
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
    ReadLayout,
    ReadTerminal {
        process_id: String,
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
        args_json: String,
    },
    AgentApprovalResolved {
        sid: String,
        call_id: String,
    },
    AgentMessagesSnapshot {
        sid: String,
        messages_json: String,
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
