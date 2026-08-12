//! Everything that only exists on a desktop.
//!
//! One gate for the lot, rather than an attribute on each declaration. The crate's public paths
//! are unchanged: `lib.rs` re-exports this module's contents, so `vmux_agent::plugin` still
//! resolves from outside and `crate::plugin` still resolves from within.

mod tree;
pub use tree::{AgentPagesPlugin, AgentPlugin, AgentSessionPlugin};

pub mod acp_install;
pub mod acp_registry;
pub mod agents;
pub mod attach;
pub mod attention;
pub mod browser_pane;
pub mod chat;
pub mod client;
pub mod command;
pub mod command_bar;
pub mod echo;
pub mod echo_plugin;
pub mod events;
pub mod exec;
pub mod follow;
pub mod handoff;
pub mod launch;
pub mod managed_mcp;
pub mod mcp;
pub mod page_open;
pub mod provider;
pub mod providers;
pub mod query;
pub mod run_state;
pub mod run_state_kind;
pub mod run_terminal;
pub mod self_command;
pub mod session;
pub mod snapshot_updater;
pub mod spawn;
pub mod strategy;
pub mod toast;
pub mod tools;
pub mod url;
pub mod workspace;

pub mod systems {
    pub mod approval;
    pub mod surface_errors;
}

#[cfg(test)]
pub mod test_support;

pub(crate) mod tidy;

pub use self::attach::{
    attach_acp_agent_to_stack, attach_page_agent_to_stack, page_agent_placeholder_url,
};
pub use self::command::AgentLookups;
pub use self::provider::AgentExecutableOverride;
pub use self::run_terminal::AgentTerminalRegions;
pub use self::spawn::detect_agent_session_process_exit;
pub use vmux_space::cwd::valid_cwd;

pub(crate) use self::follow::on_tidy_action;
pub(crate) use self::run_terminal::agent_terminal_shell;
pub(crate) use self::workspace::{
    PendingAgentChoice, PendingAgentProject, RepositoryNeedsWorktree,
};

pub use vmux_service::{http, message, stream};

pub use client::acp::AcpSession;
pub use client::cli::strategy::CliAgentStrategy;
pub use events::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
    ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
};
pub use launch::build_agent_launch;
pub use mcp::McpServerConfig;
pub use message::{AssistantBlock, Message};
pub use run_state::AgentRunState;
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
pub use toast::{AgentToast, ToastLevel};
pub use tools::mcp_tool_defs;
pub use url::{AgentKind, AgentUrl};
pub use vmux_session::room::{
    ChatRoom, CollaborativeDocument, CrdtChangeReceived, DocumentKind, MaterializedRoomEvent,
    MemberPresence, MessageDelivery, RoomAgentBinding, RoomEventIdentity, RoomEventIndex,
    RoomIndex, RoomIntent, RoomMember, RoomMessageContent, RoomMetadata, RoomOpCommitted,
    RoomOpReceived, RoomPlugin, RoomProjection, StreamingMessage,
};
pub use vmux_session::{
    AgentApprovalPolicy, AgentMessages, AgentSession, AgentVariant, PromptQueue, QueuedPrompt,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_cwd_is_accepted() {
        assert_eq!(valid_cwd("").unwrap(), None);
    }
}
