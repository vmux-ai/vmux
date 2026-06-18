#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod client;
pub mod components;
pub mod echo;
pub mod echo_plugin;
pub mod events;
pub mod exec;
pub mod launch;
pub mod mcp;
pub mod plugin;
pub mod providers;
pub mod run_state;
pub mod run_state_kind;
pub mod session;
pub mod snapshot_updater;
pub mod strategy;
pub mod systems {
    pub mod approval;
    pub mod continue_after_tool;
    pub mod dispatch_tool;
    pub mod drain_stream;
    pub mod process_input;
    pub mod surface_errors;
}
pub mod toast;
pub mod tool_dispatch;
pub mod tools;
pub mod url;
pub mod variant;

pub use vmux_service::{http, message, stream};

pub use client::cli::strategy::CliAgentStrategy;
pub use client::page::plugin::PageAgentPlugin;
pub use components::{AgentApprovalPolicy, AgentMessages, AgentSession, PendingUserInput};
pub use launch::build_agent_launch;
pub use mcp::McpServerConfig;
pub use message::{AssistantBlock, Message};
pub use plugin::AgentPlugin;
pub use run_state::{AgentRunState, ToolDispatchOutput};
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
pub use toast::{AgentToast, ToastLevel};
pub use tools::mcp_tool_defs;
pub use url::{AgentKind, AgentUrl};
pub use variant::AgentVariant;
