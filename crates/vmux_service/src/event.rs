use serde::{Deserialize, Serialize};

/// Event name for process list updates (host -> webview).
pub const PROCESSES_LIST_EVENT: &str = "processes_list";

/// Event name for process navigation (webview -> host).
pub const PROCESSES_NAVIGATE_EVENT: &str = "processes_navigate";

/// Service connection status + process list, sent periodically to the webview.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ProcessesListEvent {
    pub connected: bool,
    pub processes: Vec<ProcessEntry>,
}

/// A single service-managed process's metadata.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ProcessEntry {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub uptime_secs: u64,
    pub cpu_percent: f32,
    pub mem_bytes: u64,
    /// Whether a GUI terminal is attached to this process.
    pub attached: bool,
    /// Last few lines of terminal output for preview.
    pub preview_lines: Vec<PreviewLine>,
}

/// A simplified terminal line for the preview.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PreviewLine {
    pub text: String,
}

/// Human-readable RSS. `0` (unsampled) renders as an em dash.
pub fn format_mem(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if bytes == 0 {
        "—".to_string()
    } else if b < MB {
        "<1 MB".to_string()
    } else if b < GB {
        format!("{:.0} MB", b / MB)
    } else {
        format!("{:.1} GB", b / GB)
    }
}

/// Emitted by the processes webview when user clicks a process card.
#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ProcessNavigateEvent {
    pub process_id: String,
    /// Discriminator so serde doesn't confuse with ProcessKillEvent.
    pub navigate: bool,
}

/// Emitted to kill a single service-managed process.
#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ProcessKillEvent {
    pub process_id: String,
    /// Discriminator so serde doesn't confuse with ProcessNavigateEvent.
    pub kill: bool,
}

/// Emitted to kill all service-managed processes.
#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ProcessKillAllEvent {
    /// Discriminator field so serde doesn't match arbitrary IPC payloads.
    pub kill_all: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_mem_buckets() {
        assert_eq!(format_mem(0), "—");
        assert_eq!(format_mem(512 * 1024), "<1 MB");
        assert_eq!(format_mem(332 * 1024 * 1024), "332 MB");
        assert_eq!(format_mem(3 * 1024 * 1024 * 1024 / 2), "1.5 GB");
    }
}
