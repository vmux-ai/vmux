//! Shared scene markers and IPC payloads for vmux crates.

mod active;
pub mod input_root;
mod session;

pub use active::Active;
pub use input_root::{AppInputRoot, PREFIX_TIMEOUT_SECS, VmuxPrefixChordSet, VmuxPrefixState};
pub use session::{SessionSavePath, SessionSaveQueue};

use serde::Deserialize;

/// Payload from `window.cef.emit({ url })` (single-arg form matches bevy_cef IPC).
#[derive(Debug, Clone, Deserialize)]
pub struct WebviewDocumentUrlEmit {
    pub url: String,
}
