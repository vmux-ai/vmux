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
pub use client::page::strategy::AgentPageStrategy;
pub use components::{AgentApprovalPolicy, AgentMessages, AgentSession, PendingUserInput};
pub use echo::{EchoPageStrategy, synthetic_echo_stream};
pub use events::{
    AgentApprovalReply, AgentApprovalRequest, AgentCommandRequest, AgentDelta, AgentInput,
    AgentQueryRequest, AgentToolStatus, ApprovalDecision, ToolStatus,
};
pub use launch::build_agent_launch;
pub use mcp::McpServerConfig;
pub use message::{AssistantBlock, Message};
pub use plugin::AgentPlugin;
pub use run_state::{AgentRunState, ToolDispatchOutput};
pub use session::AgentSessionExited;
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
pub use url::{AgentKind, AgentUrl};
pub use variant::AgentVariant;
