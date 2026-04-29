use vmux_terminal::event::{TermCursor, TermLine, TermSelectionRange};

/// Unique identifier for a service-managed terminal process.
/// Stored as raw bytes for rkyv compatibility.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ProcessId(pub [u8; 16]);

impl Default for ProcessId {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessId {
    pub fn new() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }

    pub fn to_uuid(&self) -> uuid::Uuid {
        uuid::Uuid::from_bytes(self.0)
    }

    pub fn from_uuid(u: uuid::Uuid) -> Self {
        Self(*u.as_bytes())
    }
}

impl std::fmt::Display for ProcessId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_uuid())
    }
}

impl std::str::FromStr for ProcessId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_uuid(s.parse()?))
    }
}

/// Messages sent from the GUI client to the service.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ClientMessage {
    CreateProcess {
        shell: String,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    },
    AttachProcess {
        process_id: ProcessId,
    },
    DetachProcess {
        process_id: ProcessId,
    },
    ProcessInput {
        process_id: ProcessId,
        data: Vec<u8>,
    },
    ResizeProcess {
        process_id: ProcessId,
        cols: u16,
        rows: u16,
    },
    ListProcesses,
    KillProcess {
        process_id: ProcessId,
    },
    RequestSnapshot {
        process_id: ProcessId,
    },
    SetSelection {
        session_id: SessionId,
        range: Option<TermSelectionRange>,
    },
    ExtendSelectionTo {
        session_id: SessionId,
        col: u16,
        row: u16,
    },
    SelectWordAt {
        session_id: SessionId,
        col: u16,
        row: u16,
    },
    SelectLineAt {
        session_id: SessionId,
        row: u16,
    },
    GetSelectionText {
        session_id: SessionId,
    },
    Shutdown,
}

/// Messages sent from the service to the GUI client.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ServiceMessage {
    ProcessCreated {
        process_id: ProcessId,
    },
    ProcessOutput {
        process_id: ProcessId,
        data: Vec<u8>,
    },
    ViewportPatch {
        process_id: ProcessId,
        changed_lines: Vec<(u16, TermLine)>,
        cursor: TermCursor,
        cols: u16,
        rows: u16,
        selection: Option<TermSelectionRange>,
        full: bool,
    },
    ProcessExited {
        process_id: ProcessId,
        exit_code: Option<i32>,
    },
    ProcessList {
        processes: Vec<ProcessInfo>,
    },
    Snapshot {
        process_id: ProcessId,
        lines: Vec<TermLine>,
        cursor: TermCursor,
        cols: u16,
        rows: u16,
    },
    Error {
        message: String,
    },
    SelectionText {
        session_id: SessionId,
        text: String,
    },
    TerminalMode {
        session_id: SessionId,
        mouse_capture: bool,
        copy_mode: bool,
    },
}

/// Metadata about a process, returned in ProcessList.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ProcessInfo {
    pub id: ProcessId,
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub created_at_secs: u64,
}
