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

mod webview_document_emit;

pub use webview_document_emit::WebviewDocumentUrlEmit;
