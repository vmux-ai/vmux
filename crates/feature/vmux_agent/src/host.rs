mod tree;
pub use tree::{AgentPagesPlugin, AgentPlugin, AgentSessionPlugin};

#[derive(bevy::prelude::SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AgentContinuationSet;

pub mod acp_registry;
pub mod acp_tool;
pub(crate) mod approval;
pub mod attach;
pub mod attention;
mod cli;
pub mod command;
pub mod command_bar;
pub mod echo;
pub mod event;
pub mod exec;
pub mod follow;
pub mod handoff;
mod ingress;
pub mod launch;
pub mod managed_mcp;
pub mod mcp;
pub(crate) mod model;
pub mod page_open;
pub mod provider;
mod resume;
pub mod run_state_kind;
pub mod run_terminal;
pub mod runtime;
pub mod self_command;
pub mod session;
pub mod session_source;
pub mod snapshot_updater;
pub mod spawn;
pub mod toast;
mod tool;
mod transcript;
pub mod url;
pub mod workspace;

#[cfg(test)]
pub mod test_support;

pub(crate) mod tidy;

pub use vmux_space::cwd::valid_cwd;

pub(crate) use self::workspace::{
    PendingAgentChoice, PendingAgentProject, RepositoryNeedsWorktree,
};

pub use vmux_api::room as message;

pub use crate::stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
pub use cli::CliSessionSource;
pub use mcp::McpServerConfig;
pub use message::{AssistantBlock, Message};
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
pub use toast::{AgentToast, ToastLevel};
pub use tool::WorkspaceToolPlugin;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_cwd_is_accepted() {
        assert_eq!(valid_cwd("").unwrap(), None);
    }
}
