pub use vmux_api::room::{
    ApprovalRequest, ClientOpId, NewChatRequest, PromptRequest, RemoteApproval, RemoteEvent,
    RemoteMediaEntry, RemoteSession, RemoteStatus, RoomEvent, RoomId,
};

#[cfg(host)]
pub mod authorization;
#[cfg(host)]
pub mod client_operation;
#[cfg(host)]
pub mod pairing;
#[cfg(host)]
pub mod quic;
#[cfg(host)]
pub mod server;

#[cfg(host)]
pub(crate) use server::RemotePlugin;

#[cfg(host)]
pub(crate) struct PrivateFile(std::path::PathBuf);

#[cfg(host)]
impl PrivateFile {
    pub(crate) fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self(path.into())
    }

    pub(crate) fn write(&self, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        vmux_path::AtomicFile::write(&self.0, contents.as_ref())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}
