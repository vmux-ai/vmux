//! The subset of the protocol a remote peer may speak.
//!
//! Every other variant of [`ClientMessage`](super::ClientMessage) stays flat on that enum and is
//! therefore local-only: a transport that decodes into [`SharedMessage`] cannot represent
//! `CreateProcess`, `RunShell`, `Shutdown` or `SubscribeAgentCommands`, so the boundary is enforced
//! by the type system rather than by a runtime check that someone has to remember to write.
//!
//! Adding a variant to a parent enum leaves it local. Widening the remote surface is the deliberate
//! act of moving a variant in here, which is what makes the failure mode closed.

use super::{
    AcpModelOption, AgentAttachment, AgentCommand, AgentRunStatus, ApprovalDecision, ClientMessage,
    ServiceMessage,
};
use crate::room::{ClientOpId, RemoteAgent, RemoteMediaEntry, RemoteSession};

/// Operations a remote peer is permitted to perform.
///
/// Carried over the local link too, wrapped in [`ClientMessage::Shared`] — "shared" is about which
/// transports may carry it, not about where it originates.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum SharedMessage {
    /// Anything addressed to one session.
    ///
    /// The id is hoisted out of the actions so a receiver resolves the session once rather than
    /// in every arm, and so adding an action cannot accidentally forget to carry one.
    Agent { sid: String, action: AgentAction },
    /// The running sessions a client can attach to. No local equivalent — the desktop reads the
    /// registries directly rather than asking for them.
    ListSessions,
    /// Something only the GUI can answer, forwarded to it through the broker.
    AgentCommand(SharedAgentCommand),
}

/// What a client asks of one session.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentAction {
    /// Subscribe to the transcript and status.
    Attach,
    /// Submit a prompt.
    ///
    /// `attachments` is empty when there are none. Prompts with and without them used to be two
    /// variants, so every reader had to handle both in order to handle either.
    Input {
        text: String,
        context: Option<String>,
        attachments: Vec<AgentAttachment>,
    },
    /// Interrupt the in-flight turn without tearing the session down.
    Cancel,
    Approve {
        call_id: String,
        decision: ApprovalDecision,
    },
    /// Browse attachable files under `$HOME`, for this session's composer.
    ///
    /// `query` is a path fragment, resolved and confined by the daemon; a client cannot escape the
    /// home directory by crafting it.
    ListMedia { query: String },
}

impl SharedMessage {
    /// Address an action to a session.
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

/// Agent commands a remote peer is permitted to issue.
///
/// All three are answered by the GUI rather than the daemon, because only the ECS holds the
/// registry and the roster.
#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum SharedAgentCommand {
    /// Open a focused desktop tab with the default agent and submit its first prompt.
    NewAgentChat {
        /// Idempotency key. A client that retries after a dropped connection reuses it, and the
        /// daemon answers `AlreadyApplied` rather than opening a second chat.
        client_op_id: ClientOpId,
        prompt: String,
        /// Launch URL of the agent to start with; `None` uses the default.
        agent_url: Option<String>,
    },
    /// Ask the GUI for the installed-agent list. Answered as JSON in
    /// [`AgentCommandResult::Text`](super::AgentCommandResult::Text), because only the GUI holds
    /// the registry.
    ListAgents,
    /// Ask the GUI for the active space's team roster. Answered as JSON in
    /// [`AgentCommandResult::Text`](super::AgentCommandResult::Text); the roster is assembled from
    /// ECS state, and the daemon serving the remote API runs in a different process from the ECS
    /// that holds it.
    ListTeam,
}

impl From<SharedAgentCommand> for AgentCommand {
    fn from(command: SharedAgentCommand) -> Self {
        Self::Shared(command)
    }
}

/// Session events a remote peer is permitted to receive.
///
/// Everything else a session emits — terminal output, proposed diffs, process lifecycle — stays
/// flat on [`ServiceMessage`] and never leaves the machine.
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
    /// Identity reported by an ACP agent during initialization.
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
    /// Current model and selectable models reported by an ACP session.
    AcpModelInfo {
        sid: String,
        config_id: String,
        current_model_id: String,
        models: Vec<AcpModelOption>,
    },
    /// The session as it now stands. Sent when an ACP agent reports its identity, model or
    /// workspace, because the values a client renders come from daemon state it cannot see.
    /// Appended last so the preceding positional rkyv discriminants keep their values.
    Session {
        session: RemoteSession,
    },
}

impl From<SharedEvent> for ServiceMessage {
    fn from(event: SharedEvent) -> Self {
        Self::Shared(event)
    }
}

/// The answer to a [`SharedMessage`] sent on a control stream.
///
/// Typed rather than a status code plus loose JSON, which is what the HTTP path had: four of its
/// nine routes answered with a bare `StatusCode` and no body, so a client could not tell an
/// accepted prompt from a replayed one without inferring it.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum SharedResponse {
    /// Applied.
    Ok,
    /// Recognised as a replay of a `client_op_id` already seen, and deliberately not re-run.
    AlreadyApplied,
    Sessions(Vec<RemoteSession>),
    Agents(Vec<RemoteAgent>),
    Media(Vec<RemoteMediaEntry>),
    /// A GUI-held list, forwarded verbatim. Still JSON because the shape belongs to the page that
    /// renders it, and re-deriving it here would be a second place to keep in step.
    BrokerJson(String),
    /// Refused, with enough detail for a client to decide whether retrying could ever help.
    Failed(SharedFailure),
}

/// Why a request was refused.
///
/// Distinguishes the cases the HTTP status codes blurred: a client needs to know that
/// `NoDesktop` clears up when a window opens, whereas `NotFound` will not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum SharedFailure {
    /// No session with that id in either registry.
    NotFound,
    /// Malformed, oversized, or otherwise rejected before execution.
    Invalid,
    /// The GUI holds the answer and no GUI is attached. Resolves on its own when one is.
    NoDesktop,
    /// The daemon failed while handling it.
    Internal,
}
