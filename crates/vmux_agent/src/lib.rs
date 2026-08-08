//! Agent integration: pluggable CLI agent strategies (vibe, claude, codex), session
//! lifecycle and discovery, and the ECS messaging that lets agents drive screenshots,
//! recordings, browser snapshots, and layout commands.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod agents_page;
pub mod chat_page;
pub mod vibe;

#[cfg(native)]
pub mod acp_install;
#[cfg(native)]
pub mod acp_registry;
#[cfg(native)]
pub mod client;
#[cfg(native)]
pub mod components;
#[cfg(native)]
pub mod echo;
#[cfg(native)]
pub mod echo_plugin;
#[cfg(native)]
pub mod events;
#[cfg(native)]
pub mod exec;
#[cfg(native)]
pub mod handoff;
#[cfg(native)]
pub mod launch;
#[cfg(native)]
pub mod managed_mcp;
#[cfg(native)]
pub mod mcp;
#[cfg(native)]
pub mod plugin;
#[cfg(native)]
pub mod providers;
#[cfg(native)]
pub mod room;
#[cfg(native)]
pub mod run_state;
#[cfg(native)]
pub mod run_state_kind;
#[cfg(native)]
pub mod session;
#[cfg(native)]
pub mod snapshot_updater;
#[cfg(native)]
pub mod strategy;
#[cfg(native)]
mod tidy;
#[cfg(native)]
pub mod systems {
    pub mod approval;
    pub mod surface_errors;
}
#[cfg(native)]
pub mod toast;
#[cfg(native)]
pub mod tools;
#[cfg(native)]
pub mod url;
#[cfg(native)]
pub mod variant;

#[cfg(native)]
pub use vmux_service::{http, message, stream};

#[cfg(native)]
pub use client::acp::AcpSession;
#[cfg(native)]
pub use client::cli::strategy::CliAgentStrategy;
#[cfg(native)]
pub use components::{AgentApprovalPolicy, AgentMessages, AgentSession, PromptQueue, QueuedPrompt};
#[cfg(native)]
pub use events::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
    ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
};
#[cfg(native)]
pub use launch::build_agent_launch;
#[cfg(native)]
pub use mcp::McpServerConfig;
#[cfg(native)]
pub use message::{AssistantBlock, Message};
#[cfg(native)]
pub use plugin::AgentPlugin;
#[cfg(native)]
pub use room::{
    ChatRoom, CollaborativeDocument, CrdtChangeReceived, DocumentKind, MaterializedRoomEvent,
    MemberPresence, MessageDelivery, RoomAgentBinding, RoomEventIdentity, RoomEventIndex,
    RoomIndex, RoomIntent, RoomMember, RoomMessageContent, RoomMetadata, RoomOpCommitted,
    RoomOpReceived, RoomPlugin, RoomProjection, StreamingMessage,
};
#[cfg(native)]
pub use run_state::AgentRunState;
#[cfg(native)]
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
#[cfg(native)]
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
#[cfg(native)]
pub use toast::{AgentToast, ToastLevel};
#[cfg(native)]
pub use tools::mcp_tool_defs;
#[cfg(native)]
pub use url::{AgentKind, AgentUrl};
#[cfg(native)]
pub use variant::AgentVariant;
