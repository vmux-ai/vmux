//! Agent integration: pluggable CLI agent strategies (vibe, claude, codex), session
//! lifecycle and discovery, and the ECS messaging that lets agents drive screenshots,
//! recordings, browser snapshots, and layout commands.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod agents_page;
pub mod chat_page;
pub mod vibe;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod acp_install;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod acp_registry;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod client;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod components;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod echo;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod echo_plugin;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod events;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod exec;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod handoff;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod launch;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod managed_mcp;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod mcp;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod plugin;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod providers;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod room;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod run_state;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod run_state_kind;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod session;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod snapshot_updater;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod strategy;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
mod tidy;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod systems {
    pub mod approval;
    pub mod surface_errors;
}
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod toast;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod tools;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod url;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod variant;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use vmux_service::{http, message, stream};

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use client::acp::AcpSession;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use client::cli::strategy::CliAgentStrategy;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use components::{AgentApprovalPolicy, AgentMessages, AgentSession, PromptQueue, QueuedPrompt};
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use events::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
    ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
};
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use launch::build_agent_launch;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use mcp::McpServerConfig;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use message::{AssistantBlock, Message};
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use plugin::AgentPlugin;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use room::{
    ChatRoom, CollaborativeDocument, CrdtChangeReceived, DocumentKind, MaterializedRoomEvent,
    MemberPresence, MessageDelivery, RoomAgentBinding, RoomEventIdentity, RoomEventIndex,
    RoomIndex, RoomIntent, RoomMember, RoomMessageContent, RoomMetadata, RoomOpCommitted,
    RoomOpReceived, RoomPlugin, RoomProjection, StreamingMessage,
};
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use run_state::AgentRunState;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use toast::{AgentToast, ToastLevel};
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use tools::mcp_tool_defs;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use url::{AgentKind, AgentUrl};
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use variant::AgentVariant;
