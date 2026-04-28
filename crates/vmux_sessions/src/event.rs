use serde::{Deserialize, Serialize};

/// Event name for session list updates (host -> webview).
pub const SESSIONS_LIST_EVENT: &str = "sessions_list";

/// Event name for session navigation (webview -> host).
pub const SESSIONS_NAVIGATE_EVENT: &str = "sessions_navigate";

/// URL for the sessions monitor webview.
pub const SESSIONS_WEBVIEW_URL: &str = "vmux://sessions/";

/// Daemon connection status + session list, sent periodically to the webview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionsListEvent {
    pub connected: bool,
    pub sessions: Vec<SessionEntry>,
}

/// A single daemon session's metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEntry {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub uptime_secs: u64,
    /// Whether a GUI terminal is attached to this session.
    pub attached: bool,
    /// Last few lines of terminal output for preview.
    pub preview_lines: Vec<PreviewLine>,
}

/// A simplified terminal line for the preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewLine {
    pub text: String,
}

/// Emitted by the sessions webview when user clicks a session card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionNavigateEvent {
    pub session_id: String,
}

/// Emitted to kill a single daemon session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionKillEvent {
    pub session_id: String,
}

/// Emitted to kill all daemon sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionKillAllEvent {
    /// Discriminator field so serde doesn't match arbitrary IPC payloads.
    pub kill_all: bool,
}
