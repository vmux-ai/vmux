mod tree;
pub use tree::AgentPlugin;

pub mod acp;
pub use acp as acp_tool;
pub use acp::registry as acp_registry;
mod approval;
pub mod attach;
pub mod attention;
pub mod command;
pub mod command_bar;
mod continuation;
pub mod event;
pub mod follow;
mod handoff;
mod ingress;
mod model_selection;
mod page;
pub mod run_state_kind;
pub mod runtime;
pub mod snapshot;
pub mod toast;
mod transcript;
pub mod url;

#[cfg(test)]
pub mod test_support;

mod tidy;

pub use vmux_api::room as message;

pub use crate::mcp::McpServerConfig;
pub use message::{AssistantBlock, Message};
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
pub use toast::{AgentToast, ToastLevel};
pub use url::AgentUrl;
pub use vmux_session::room::{
    ChatRoom, CollaborativeDocument, CrdtChangeReceived, DocumentKind, MaterializedRoomEvent,
    MemberPresence, MessageDelivery, RoomAgentBinding, RoomEventIdentity, RoomMember,
    RoomMessageContent, RoomMetadata, RoomOpCommitted, RoomOpReceived, RoomPlugin, RoomProjection,
    StreamingMessage,
};
pub use vmux_session::{AcpSession, AgentApprovalPolicy, AgentMessages, PromptQueue, QueuedPrompt};
