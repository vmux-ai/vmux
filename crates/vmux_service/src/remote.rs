pub use vmux_remote::{
    ApprovalRequest, ClientOpId, NewChatRequest, PromptRequest, RemoteApproval, RemoteEvent,
    RemoteMediaEntry, RemoteSession, RemoteStatus, RoomEvent, RoomId,
};

#[cfg(not(target_arch = "wasm32"))]
pub mod server;
