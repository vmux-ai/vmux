//! Shared scene markers and IPC payloads for vmux crates.

mod active;
pub mod command_palette;
pub mod input_root;
mod navigation_history;
pub mod pane_corner_clip;
mod session;

pub use active::Active;
pub use command_palette::VmuxCommandPaletteState;
pub use input_root::{AppInputRoot, PREFIX_TIMEOUT_SECS, VmuxPrefixChordSet, VmuxPrefixState};
pub use navigation_history::{
    favicon_url_for_page_url, page_host_for_favicon_url, NavigationHistory, NavigationHistoryEntry,
    NavigationHistoryFile,
};
pub use session::{
    NavigationHistoryPath, NavigationHistorySaveQueue, SessionSavePath, SessionSaveQueue,
};

use serde::Deserialize;

/// Payload from `window.cef.emit(...)` (single JSON object). Preload uses `{ url }`; history UI uses `{ vmux_open_in_pane }`, etc.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WebviewDocumentUrlEmit {
    #[serde(default)]
    pub url: Option<String>,
    /// When set (e.g. from history pane), open this URL in the active main pane.
    #[serde(default, rename = "vmux_open_in_pane")]
    pub vmux_open_in_pane: Option<String>,
    /// History pane asks the host to push the current list (after `cef.listen` is registered).
    #[serde(default, rename = "vmux_request_history")]
    pub vmux_request_history: bool,
    /// Echoed on the next `vmux_history` host emit so the UI can confirm the bridge delivered (`u32` so JS numbers stay exact).
    #[serde(default, rename = "vmux_history_sync_nonce")]
    pub vmux_history_sync_nonce: Option<u32>,
    /// History pane asks the host to wipe persisted visit list.
    #[serde(default, rename = "vmux_clear_history")]
    pub vmux_clear_history: bool,
}
