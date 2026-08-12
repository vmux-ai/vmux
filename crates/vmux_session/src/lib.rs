//! The ECS model of an agent session: its descriptor, transcript, approval policy and prompt
//! queue, plus the room each session projects into.
//!
//! Deliberately free of `bevy_cef` and of anything that renders, so the server can own this
//! state whether or not a window exists. That seam is a crate boundary rather than a feature,
//! because features unify per package across an invocation and would relink CEF into a headless
//! build without failing anything.

pub mod acp;
pub mod session;
pub mod variant;

pub use acp::AcpSession;
pub use session::{
    AgentApprovalPolicy, AgentConversationTitle, AgentMessages, AgentSession, PromptQueue,
    QueuedPrompt, approval_tool_key, provisional_conversation_title,
};
pub use variant::AgentVariant;
