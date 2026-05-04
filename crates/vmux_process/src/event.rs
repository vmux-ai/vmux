use serde::{Deserialize, Serialize};

/// Event name for process list updates (host -> webview).
pub const PROCESSES_LIST_EVENT: &str = "processes_list";

/// Event name for process navigation (webview -> host).
pub const PROCESSES_NAVIGATE_EVENT: &str = "processes_navigate";

/// URL for the processes monitor webview.
pub const PROCESSES_WEBVIEW_URL: &str = "vmux://services/";

/// Service connection status + process list, sent periodically to the webview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessesListEvent {
    pub connected: bool,
    pub processes: Vec<ProcessEntry>,
}

/// A single service-managed process's metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessEntry {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub uptime_secs: u64,
    /// Whether a GUI terminal is attached to this process.
    pub attached: bool,
    /// Last few lines of terminal output for preview.
    pub preview_lines: Vec<PreviewLine>,
}

/// A simplified terminal line for the preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewLine {
    pub text: String,
}

/// Emitted by the processes webview when user clicks a process card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessNavigateEvent {
    pub process_id: String,
    /// Discriminator so serde doesn't confuse with ProcessKillEvent.
    pub navigate: bool,
}

/// Emitted to kill a single service-managed process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessKillEvent {
    pub process_id: String,
    /// Discriminator so serde doesn't confuse with ProcessNavigateEvent.
    pub kill: bool,
}

/// Emitted to kill all service-managed processes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessKillAllEvent {
    /// Discriminator field so serde doesn't match arbitrary IPC payloads.
    pub kill_all: bool,
}
