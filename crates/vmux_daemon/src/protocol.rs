use vmux_terminal::event::{TermCursor, TermLine, TermSelectionRange};

/// Unique identifier for a daemon-managed terminal session.
/// Stored as raw bytes for rkyv compatibility.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct SessionId(pub [u8; 16]);

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionId {
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

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_uuid())
    }
}

impl std::str::FromStr for SessionId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_uuid(s.parse()?))
    }
}

/// Messages sent from the GUI client to the daemon.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ClientMessage {
    CreateSession {
        shell: String,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    },
    AttachSession {
        session_id: SessionId,
    },
    DetachSession {
        session_id: SessionId,
    },
    SessionInput {
        session_id: SessionId,
        data: Vec<u8>,
    },
    ResizeSession {
        session_id: SessionId,
        cols: u16,
        rows: u16,
    },
    ListSessions,
    KillSession {
        session_id: SessionId,
    },
    RequestSnapshot {
        session_id: SessionId,
    },
    Shutdown,
}

/// Messages sent from the daemon to the GUI client.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum DaemonMessage {
    SessionCreated {
        session_id: SessionId,
    },
    SessionOutput {
        session_id: SessionId,
        data: Vec<u8>,
    },
    ViewportPatch {
        session_id: SessionId,
        changed_lines: Vec<(u16, TermLine)>,
        cursor: TermCursor,
        cols: u16,
        rows: u16,
        selection: Option<TermSelectionRange>,
        full: bool,
    },
    SessionExited {
        session_id: SessionId,
        exit_code: Option<i32>,
    },
    SessionList {
        sessions: Vec<SessionInfo>,
    },
    Snapshot {
        session_id: SessionId,
        lines: Vec<TermLine>,
        cursor: TermCursor,
        cols: u16,
        rows: u16,
    },
    Error {
        message: String,
    },
}

/// Metadata about a session, returned in SessionList.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub created_at_secs: u64,
}
