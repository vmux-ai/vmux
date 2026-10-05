pub use vmux_api::room::{
    ApprovalRequest, ClientOpId, NewChatRequest, PromptRequest, RemoteApproval, RemoteEvent,
    RemoteMediaEntry, RemoteSession, RemoteStatus, RoomEvent, RoomId,
};

#[cfg(host)]
pub(crate) use server::RemotePlugin;

#[cfg(host)]
pub mod authorization;
#[cfg(host)]
mod authorization_driver;
#[cfg(host)]
pub mod client_operation;
#[cfg(host)]
mod client_operation_driver;
#[cfg(host)]
mod exposure_driver;
#[cfg(host)]
mod file_driver;
#[cfg(host)]
pub mod pairing;
#[cfg(host)]
mod pairing_driver;
#[cfg(host)]
pub mod quic;
#[cfg(host)]
mod quic_dialer;
#[cfg(host)]
pub mod server;
