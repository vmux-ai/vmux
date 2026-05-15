pub mod claude;
pub mod cli_trait;
pub mod codex;
pub mod components;
pub mod echo;
pub mod events;
pub mod exec;
pub mod gui;
pub mod gui_plugin;
pub mod kind;
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
pub mod variant;
pub mod vibe;

pub use cli_trait::CliAgentStrategy;
pub use components::{AgentApprovalPolicy, AgentMessages, AgentSession, PendingUserInput};
pub use echo::{EchoGuiStrategy, synthetic_echo_stream};
pub use events::{
    AgentApprovalReply, AgentApprovalRequest, AgentDelta, AgentInput, AgentToolStatus,
    ApprovalDecision, ToolStatus,
};
pub use gui::GuiAgentStrategy;
pub use gui_plugin::GuiAgentPlugin;
pub use kind::AgentKind;
pub use kind::AgentUrl;
pub use mcp::McpServerConfig;
pub use message::{AssistantBlock, Message};
pub use plugin::AgentSessionPlugin;
pub use run_state::{AgentRunState, ToolDispatchOutput};
pub use session::AgentSessionExited;
pub use strategy::BoxedStrategy;
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
pub use variant::AgentVariant;
