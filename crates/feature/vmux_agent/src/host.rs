mod tree;
pub use tree::{AgentPagesPlugin, AgentPlugin, AgentSessionPlugin};

pub mod acp;
pub use acp as acp_tool;
pub use acp::registry as acp_registry;
pub(crate) mod approval;
pub mod attach;
pub mod attention;
mod cli;
pub mod command;
pub mod command_bar;
pub mod echo;
pub mod event;
pub mod follow;
pub mod handoff;
mod ingress;
pub mod launch;
mod model_selection;
pub mod page_open;
pub mod provider;
pub mod run_state_kind;
pub mod runtime;
pub mod session;
pub mod snapshot;
pub mod spawn;
pub mod toast;
mod tool;
mod transcript;
pub mod url;

#[cfg(test)]
pub mod test_support;

pub(crate) mod tidy;

pub use vmux_api::room as message;

pub use crate::mcp::McpServerConfig;
pub use crate::stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
pub use cli::CliSessionSource;
pub use message::{AssistantBlock, Message};
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
pub use toast::{AgentToast, ToastLevel};
pub use tool::AgentToolPlugin;
pub use url::{AgentKind, AgentUrl};
pub use vmux_session::room::{
    ChatRoom, CollaborativeDocument, CrdtChangeReceived, DocumentKind, MaterializedRoomEvent,
    MemberPresence, MessageDelivery, RoomAgentBinding, RoomEventIdentity, RoomMember,
    RoomMessageContent, RoomMetadata, RoomOpCommitted, RoomOpReceived, RoomPlugin, RoomProjection,
    StreamingMessage,
};
pub use vmux_session::{
    AcpSession, AgentApprovalPolicy, AgentMessages, AgentSession, AgentVariant, PromptQueue,
    QueuedPrompt,
};
