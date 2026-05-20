#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod client;
pub mod components;
pub mod echo;
pub mod events;
pub mod exec;
pub mod launch;
pub mod mcp;
pub mod message;
pub mod plugin;
pub mod run_state;
pub mod session;
pub mod snapshot_updater;
pub mod strategy;
pub mod stream;
pub mod systems {
    pub mod approval;
    pub mod dispatch_tool;
    pub mod drain_stream;
    pub mod process_input;
}
pub mod url;
pub mod variant;

pub use client::cli::strategy::CliAgentStrategy;
pub use client::page::agent::AgentPage;
pub use components::{AgentApprovalPolicy, AgentMessages, AgentSession, PendingUserInput};
pub use launch::build_agent_launch;
pub use mcp::McpServerConfig;
pub use message::AssistantBlock;
pub use plugin::AgentPlugin;
pub use run_state::AgentRunState;
pub use url::{AgentKind, AgentUrl};
pub use variant::AgentVariant;
