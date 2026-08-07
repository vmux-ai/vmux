pub use vmux_wire::room::{
    ApprovalRequest, ClientOpId, NewChatRequest, PromptRequest, RemoteApproval, RemoteEvent,
    RemoteMediaEntry, RemoteSession, RemoteStatus, RoomEvent, RoomId,
};

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod quic;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod server;
