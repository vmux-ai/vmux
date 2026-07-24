pub use vmux_remote::{
    ApprovalRequest, NewChatRequest, PromptRequest, RemoteApproval, RemoteEvent, RemoteSession,
    RemoteStatus,
};

#[cfg(not(target_arch = "wasm32"))]
pub mod server;
