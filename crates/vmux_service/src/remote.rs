pub use vmux_wire::room::{
    ApprovalRequest, ClientOpId, NewChatRequest, PromptRequest, RemoteApproval, RemoteEvent,
    RemoteMediaEntry, RemoteSession, RemoteStatus, RoomEvent, RoomId,
};

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod quic;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod server;

/// Write a secret at `0600` from the start rather than narrowing after.
///
/// `fs::write` creates with the process umask and only then gets chmod'd, leaving a window where
/// another local user can read it. Recreated rather than truncated because `mode` only applies
/// when the file is created, so writing over one left behind with wider permissions would keep
/// them.
pub(crate) fn write_private(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents.as_bytes())
    }
    #[cfg(not(unix))]
    std::fs::write(path, contents)
}
