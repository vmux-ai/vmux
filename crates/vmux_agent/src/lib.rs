//! Agent integration: pluggable CLI agent strategies (vibe, claude, codex), session
//! lifecycle and discovery, and the ECS messaging that lets agents drive screenshots,
//! recordings, browser snapshots, and layout commands.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod agents_page;
pub mod chat_page;
pub mod vibe;

#[cfg(not(target_arch = "wasm32"))]
pub mod acp_install;
#[cfg(not(target_arch = "wasm32"))]
pub mod acp_registry;
#[cfg(not(target_arch = "wasm32"))]
pub mod client;
#[cfg(not(target_arch = "wasm32"))]
pub mod components;
#[cfg(not(target_arch = "wasm32"))]
pub mod echo;
#[cfg(not(target_arch = "wasm32"))]
pub mod echo_plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod events;
#[cfg(not(target_arch = "wasm32"))]
pub mod exec;
#[cfg(not(target_arch = "wasm32"))]
pub mod launch;
#[cfg(not(target_arch = "wasm32"))]
pub mod mcp;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod providers;
#[cfg(not(target_arch = "wasm32"))]
pub mod run_state;
#[cfg(not(target_arch = "wasm32"))]
pub mod run_state_kind;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot_updater;
#[cfg(not(target_arch = "wasm32"))]
pub mod strategy;
#[cfg(not(target_arch = "wasm32"))]
mod tidy;
#[cfg(not(target_arch = "wasm32"))]
pub mod systems {
    pub mod approval;
    pub mod surface_errors;
}
#[cfg(not(target_arch = "wasm32"))]
pub mod toast;
#[cfg(not(target_arch = "wasm32"))]
pub mod tools;
#[cfg(not(target_arch = "wasm32"))]
pub mod url;
#[cfg(not(target_arch = "wasm32"))]
pub mod variant;

#[cfg(not(target_arch = "wasm32"))]
pub use vmux_service::{http, message, stream};

#[cfg(not(target_arch = "wasm32"))]
pub use agents_page::AgentsManagerPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use chat_page::AgentChatPagePlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use client::acp::{AcpAgentPlugin, AcpSession};
#[cfg(not(target_arch = "wasm32"))]
pub use client::cli::strategy::CliAgentStrategy;
#[cfg(not(target_arch = "wasm32"))]
pub use client::page::plugin::PageAgentPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use components::{AgentApprovalPolicy, AgentMessages, AgentSession, PromptQueue};
#[cfg(not(target_arch = "wasm32"))]
pub use events::{
    BrowserScrollRequest, BrowserSnapshotRequest, BrowserSnapshotResponse, NavAwaitingSnapshot,
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
    ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
};
#[cfg(not(target_arch = "wasm32"))]
pub use launch::build_agent_launch;
#[cfg(not(target_arch = "wasm32"))]
pub use mcp::McpServerConfig;
#[cfg(not(target_arch = "wasm32"))]
pub use message::{AssistantBlock, Message};
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::AgentPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use run_state::AgentRunState;
#[cfg(not(target_arch = "wasm32"))]
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
#[cfg(not(target_arch = "wasm32"))]
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
#[cfg(not(target_arch = "wasm32"))]
pub use toast::{AgentToast, ToastLevel};
#[cfg(not(target_arch = "wasm32"))]
pub use tools::mcp_tool_defs;
#[cfg(not(target_arch = "wasm32"))]
pub use url::{AgentKind, AgentUrl};
#[cfg(not(target_arch = "wasm32"))]
pub use variant::AgentVariant;
