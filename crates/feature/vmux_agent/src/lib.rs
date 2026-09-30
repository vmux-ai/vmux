#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(feature = "app")]
pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");
#[cfg(all(host, feature = "app"))]
pub(crate) type Feature = host::AgentToolPlugin;

pub mod setup;

#[cfg(all(host, feature = "service"))]
pub mod acp;
#[cfg(all(host, feature = "service"))]
pub mod broker;

#[cfg(all(host, feature = "app"))]
mod cli;
#[cfg(all(host, feature = "app"))]
pub mod host;
#[cfg(all(host, feature = "app"))]
pub mod managed_mcp;
#[cfg(all(host, feature = "app"))]
mod manifest;
#[cfg(all(host, feature = "app"))]
pub mod mcp;
#[cfg(all(host, feature = "app"))]
pub use cli::AgentCliPlugin;
#[cfg(all(test, host, feature = "app"))]
pub use host::test_support;
#[cfg(all(host, feature = "app"))]
pub use host::{
    AcpSession, AgentApprovalPolicy, AgentKind, AgentMessages, AgentPagesPlugin, AgentPlugin,
    AgentRunStateKind, AgentSessionPlugin, AgentToast, AgentToolPlugin, AgentUrl, AssistantBlock,
    ChatRoom, CliSessionSource, CollaborativeDocument, CrdtChangeReceived, DocumentKind,
    LastRunStateKind, MaterializedRoomEvent, MemberPresence, Message, MessageDelivery, PromptQueue,
    QueuedPrompt, RoomAgentBinding, RoomEventIdentity, RoomMember, RoomMessageContent,
    RoomMetadata, RoomOpCommitted, RoomOpReceived, RoomPlugin, RoomProjection, StreamingMessage,
    ToastLevel, acp_registry, acp_tool, attach, attention, command, command_bar, event, follow,
    handoff, launch, message, page_open, run_state_kind, runtime, session, snapshot, spawn, toast,
    url,
};
#[cfg(all(host, feature = "app"))]
pub use mcp::McpServerConfig;
