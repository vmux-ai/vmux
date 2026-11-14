use super::{
    AgentCommandResult, AgentQueryResult, AgentRequest, AgentRequestId, CommandLifecycleKind,
    CopyModeKey, JsonValue, ProcessInfo, SharedEvent, SharedMessage,
};
use crate::{ProcessId, TermCursor, TermLine, TermSelectionRange};

#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ClientMessage {
    CreateProcess {
        process_id: ProcessId,
        command: String,
        args: Vec<String>,
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
    MouseWheel {
        process_id: ProcessId,
        up: bool,
        col: u16,
        row: u16,
        modifiers: u8,
    },
    ScrollWindow {
        process_id: ProcessId,
        top_row: u32,
        follow: bool,
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
        process_id: ProcessId,
        range: Option<TermSelectionRange>,
    },
    ExtendSelectionTo {
        process_id: ProcessId,
        col: u16,
        row: u16,
    },
    SelectWordAt {
        process_id: ProcessId,
        col: u16,
        row: u16,
    },
    SelectLineAt {
        process_id: ProcessId,
        row: u16,
    },
    GetSelectionText {
        process_id: ProcessId,
    },
    EnterCopyMode {
        process_id: ProcessId,
    },
    ExitCopyMode {
        process_id: ProcessId,
    },
    CopyModeKey {
        process_id: ProcessId,
        key: CopyModeKey,
    },
    SubscribeAgentCommands,
    AgentRequest {
        request_id: AgentRequestId,
        anchor: Option<ProcessId>,
        request: AgentRequest,
    },
    Shutdown,
    AgentQuery {
        request_id: AgentRequestId,
        query: AgentRequest,
    },
    AgentQueryResult(AgentQueryResult),
    AgentCommandResponse {
        request_id: AgentRequestId,
        result: AgentCommandResult,
    },
    DetachAgentSession {
        sid: String,
    },
    AcpSetSessionConfig {
        sid: String,
        request_id: u64,
        config_id: Option<String>,
        value: String,
    },
    CloseAgentSession {
        sid: String,
    },
    AgentToolResult {
        request_id: AgentRequestId,
        content: String,
        is_error: bool,
    },
    SpawnAcpAgent {
        sid: String,
        agent_id: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: String,
        anchor: ProcessId,
        mcp_command: Option<String>,
        mcp_args: Vec<String>,
        resume_acp_session_id: Option<String>,
    },
    Status,
    RebindAcpWorkspace {
        sid: String,
        cwd: String,
    },
    Shared(SharedMessage),
    ListAcpSessions {
        request_id: u64,
        agent_id: String,
        agent_name: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: String,
    },
    CancelAcpSessionCatalog {
        request_id: u64,
    },
    ChangeProcessDirectory {
        process_id: ProcessId,
        cwd: String,
        path: String,
    },
    ListProcessDirectories {
        process_id: ProcessId,
        cwd: String,
        path: String,
    },
    },
    AuthenticateAcpAgent {
        sid: String,
        method_id: String,
