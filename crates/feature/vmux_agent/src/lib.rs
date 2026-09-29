#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod setup;

#[cfg(all(host, feature = "service"))]
pub mod acp;
#[cfg(all(host, feature = "provider"))]
pub mod http;
#[cfg(all(host, feature = "provider"))]
pub mod provider;
#[cfg(all(host, feature = "service"))]
pub mod service;
#[cfg(all(host, feature = "provider"))]
pub mod stream;

#[cfg(all(host, feature = "app"))]
mod cli;
#[cfg(all(host, feature = "app"))]
pub mod host;
#[cfg(all(host, feature = "app"))]
pub use cli::AgentCliPlugin;
#[cfg(all(test, host, feature = "app"))]
pub use host::test_support;
#[cfg(all(host, feature = "app"))]
pub use host::{
    AcpSession, AgentApprovalPolicy, AgentKind, AgentMessages, AgentPagesPlugin, AgentPlugin,
    AgentRunStateKind, AgentSession, AgentSessionPlugin, AgentToast, AgentToolPlugin, AgentUrl,
    AgentVariant, AssistantBlock, ChatRoom, CliSessionSource, CollaborativeDocument,
    CrdtChangeReceived, DocumentKind, LastRunStateKind, MaterializedRoomEvent, McpServerConfig,
    MemberPresence, Message, MessageDelivery, PartialToolUse, PromptQueue, QueuedPrompt,
    RoomAgentBinding, RoomEventIdentity, RoomMember, RoomMessageContent, RoomMetadata,
    RoomOpCommitted, RoomOpReceived, RoomPlugin, RoomProjection, StopReason, StreamEvent,
    StreamingMessage, ToastLevel, ToolDef, acp_registry, acp_tool, attach, attention, command,
    command_bar, echo, event, exec, follow, handoff, launch, managed_mcp, mcp, message, page_open,
    run_state_kind, run_terminal, runtime, self_command, session, session_source, snapshot_updater,
    spawn, toast, url, valid_cwd, workspace,
};
