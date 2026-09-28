mod tree;
pub use tree::{AgentPagesPlugin, AgentPlugin, AgentSessionPlugin};

#[derive(bevy::prelude::SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AgentContinuationSet;

pub mod acp_registry;
pub mod acp_tool;
pub(crate) mod approval;
pub mod attach;
pub mod attention;
mod capture_tool;
pub mod chat;
mod cli;
pub mod command;
pub mod command_bar;
mod composer;
pub mod echo;
pub mod echo_plugin;
pub mod event;
pub mod exec;
pub mod follow;
pub mod handoff;
pub mod http;
mod ingress;
pub mod launch;
pub mod managed_mcp;
pub mod mcp;
pub(crate) mod model;
pub mod page_open;
mod prompt;
pub mod provider;
pub mod providers;
pub mod query;
mod resume;
pub mod run_state;
pub mod run_state_kind;
pub mod run_terminal;
pub mod runtime;
pub mod self_command;
pub mod session;
pub mod snapshot_updater;
pub mod spawn;
pub mod strategy;
pub mod stream;
pub mod toast;
mod tool;
mod transcript;
pub mod url;
pub mod workspace;

#[cfg(test)]
pub mod test_support;

pub(crate) mod tidy;

pub use self::attach::{
    attach_acp_agent_to_stack, attach_page_agent_to_stack, page_agent_placeholder_url,
};
pub use capture_tool::CaptureToolPlugin;
pub use vmux_space::cwd::valid_cwd;

pub(crate) use self::run_terminal::agent_terminal_shell;
pub(crate) use self::workspace::{
    PendingAgentChoice, PendingAgentProject, RepositoryNeedsWorktree,
};

pub use vmux_api::room as message;

pub use cli::CliAgentStrategy;
pub use event::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
    ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
};
pub(crate) use launch::build_agent_launch;
pub use mcp::McpServerConfig;
pub use message::{AssistantBlock, Message};
pub use run_state::AgentRunState;
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
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
