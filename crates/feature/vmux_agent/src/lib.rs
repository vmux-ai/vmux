#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(all(host, feature = "app"))]
pub(crate) struct Feature;

#[cfg(all(host, feature = "app"))]
impl vmux_core::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(all(host, feature = "service"))]
pub mod acp;
#[cfg(all(host, feature = "service"))]
pub mod broker;

#[cfg(all(host, feature = "app"))]
pub mod host;
#[cfg(all(host, feature = "app"))]
pub mod managed_mcp;
#[cfg(all(host, feature = "app"))]
pub mod mcp;
#[cfg(all(host, feature = "app"))]
mod policy;
#[cfg(all(test, host, feature = "app"))]
pub use host::test_support;
#[cfg(all(host, feature = "app"))]
pub use host::{
    AcpSession, AgentApprovalPolicy, AgentMessages, AgentPlugin, AgentRunStateKind, AgentToast,
    AgentUrl, AssistantBlock, ChatRoom, CollaborativeDocument, CrdtChangeReceived, DocumentKind,
    LastRunStateKind, MaterializedRoomEvent, MemberPresence, Message, MessageDelivery, PromptQueue,
    QueuedPrompt, RoomAgentBinding, RoomEventIdentity, RoomMember, RoomMessageContent,
    RoomMetadata, RoomOpCommitted, RoomOpReceived, RoomPlugin, RoomProjection, StreamingMessage,
    ToastLevel, acp_registry, acp_tool, attach, attention, command, command_bar, event, follow,
    message, page_open, run_state_kind, runtime, snapshot, toast, url,
};
#[cfg(all(host, feature = "app"))]
pub use mcp::McpServerConfig;
