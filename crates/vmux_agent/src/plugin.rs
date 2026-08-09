//! Everything that only exists on a desktop.
//!
//! One gate for the lot, rather than an attribute on each declaration. The crate's public paths
//! are unchanged: `lib.rs` re-exports this module's contents, so `vmux_agent::plugin` still
//! resolves from outside and `crate::plugin` still resolves from within.

pub mod agents;
pub mod chat;

pub mod acp_install;
pub mod acp_registry;
pub mod client;
pub mod command_bar;
pub mod components;
pub mod echo;
pub mod echo_plugin;
pub mod events;
pub mod exec;
pub mod handoff;
pub mod launch;
pub mod managed_mcp;
pub mod mcp;
pub mod root;
pub use root::*;
pub mod providers;
pub mod room;
pub mod run_state;
pub mod run_state_kind;
pub mod session;
pub mod snapshot_updater;
pub mod strategy;
pub mod toast;
pub mod tools;
pub mod url;
pub mod variant;

pub mod systems {
    pub mod approval;
    pub mod surface_errors;
}

pub(crate) mod tidy;

pub use vmux_service::{http, message, stream};

pub use client::acp::AcpSession;
pub use client::cli::strategy::CliAgentStrategy;
pub use components::{AgentApprovalPolicy, AgentMessages, AgentSession, PromptQueue, QueuedPrompt};
pub use events::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
    ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
};
pub use launch::build_agent_launch;
pub use mcp::McpServerConfig;
pub use message::{AssistantBlock, Message};
pub use room::{
    ChatRoom, CollaborativeDocument, CrdtChangeReceived, DocumentKind, MaterializedRoomEvent,
    MemberPresence, MessageDelivery, RoomAgentBinding, RoomEventIdentity, RoomEventIndex,
    RoomIndex, RoomIntent, RoomMember, RoomMessageContent, RoomMetadata, RoomOpCommitted,
    RoomOpReceived, RoomPlugin, RoomProjection, StreamingMessage,
};
pub use root::AgentPlugin;
pub use run_state::AgentRunState;
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
pub use toast::{AgentToast, ToastLevel};
pub use tools::mcp_tool_defs;
pub use url::{AgentKind, AgentUrl};
pub use variant::AgentVariant;
