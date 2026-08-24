use serde::{Deserialize, Serialize};

pub const PROCESSES_LIST_EVENT: &str = "processes_list";

pub const PROCESSES_NAVIGATE_EVENT: &str = "processes_navigate";

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
    pub attached: bool,
    pub preview_lines: Vec<PreviewLine>,
}

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

#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ProcessNavigateEvent {
    pub process_id: String,
    pub navigate: bool,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ProcessKillEvent {
    pub process_id: String,
    pub kill: bool,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ProcessKillAllEvent {
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
