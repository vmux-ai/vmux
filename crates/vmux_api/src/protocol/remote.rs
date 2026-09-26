use super::{
    AgentAttachment, AgentBookmarks, AgentCommand, AgentCommandExit, AgentCommandResult,
    AgentCommandTool, AgentImage, AgentQuery, AgentRecording, AgentRequest, AgentRequestId,
    AgentRunCompletion, AgentSpace, CommandLifecycleKind, CopyModeKey, JsonValue, ManagedMcpServer,
    ProcessInfo, SharedEvent, SharedMessage,
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
    AgentCommand {
        request_id: AgentRequestId,
        anchor: Option<ProcessId>,
        command: AgentCommand,
    },
    Shutdown,
    AgentQuery {
        request_id: AgentRequestId,
        query: AgentQuery,
    },
    AgentLayoutResult {
        request_id: AgentRequestId,
        result: Result<crate::protocol::layout::LayoutSnapshot, String>,
    },
    AgentTerminalReadResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentTerminalReadFullResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentCommandExitResult {
        request_id: AgentRequestId,
        result: Result<AgentCommandExit, String>,
    },
    AgentRunCompletionResult {
        request_id: AgentRequestId,
        result: Result<AgentRunCompletion, String>,
    },
    AgentSettingsResult {
        request_id: AgentRequestId,
        result: Result<JsonValue, String>,
    },
    AgentSpacesResult {
        request_id: AgentRequestId,
        result: Result<Vec<AgentSpace>, String>,
    },
    AgentScreenshotResult {
        request_id: AgentRequestId,
        result: Result<AgentImage, String>,
    },
    AgentBrowserSnapshotResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentBrowserScrollResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentRecordStartResult {
        request_id: AgentRequestId,
        result: Result<u32, String>,
    },
    AgentRecordStopResult {
        request_id: AgentRequestId,
        result: Result<AgentRecording, String>,
    },
    AgentBookmarksResult {
        request_id: AgentRequestId,
        result: Result<AgentBookmarks, String>,
    },
    AgentSimulatorScreenshotResult {
        request_id: AgentRequestId,
        result: Result<AgentImage, String>,
    },
    AgentSimulatorControlResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentWorkingDirectoryResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentVaultStatusResult {
        request_id: AgentRequestId,
        result: Result<crate::vault::VaultStatusSnapshot, String>,
    },
    AgentCommandsResult {
        request_id: AgentRequestId,
        result: Result<Vec<AgentCommandTool>, String>,
    },
    AgentCommandResponse {
        request_id: AgentRequestId,
        result: AgentCommandResult,
    },
    SpawnPageAgent {
        sid: String,
        provider: String,
        model: String,
        cwd: String,
        auto_tools: Vec<String>,
        tools_json: String,
    },
    DetachPageAgent {
        sid: String,
    },
    AcpSetModel {
        sid: String,
        request_id: u64,
        config_id: String,
        model_id: String,
    },
    ClosePageAgent {
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
        managed_mcp_servers: Vec<ManagedMcpServer>,
        effort: Option<String>,
    },
    AcpSetMode {
        sid: String,
        request_id: u64,
        config_id: String,
        mode_id: String,
    },
    Status,
    RebindAcpWorkspace {
        sid: String,
        cwd: String,
    },
    Shared(SharedMessage),
}

impl ClientMessage {
    pub fn agent_input(
        sid: String,
        text: String,
        context: Option<String>,
        attachments: Vec<AgentAttachment>,
    ) -> Self {
        SharedMessage::agent(
            sid,
            AgentRequest::Input {
                text,
                context,
                attachments,
                preferred_mode: None,
            },
        )
        .into()
    }

    pub fn agent_input_with_mode(
        sid: String,
        text: String,
        context: Option<String>,
        attachments: Vec<AgentAttachment>,
        preferred_mode: Option<String>,
    ) -> Self {
        SharedMessage::agent(
            sid,
            AgentRequest::Input {
                text,
                context,
                attachments,
                preferred_mode,
            },
        )
        .into()
    }
}

#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ServiceMessage {
    ProcessCreated {
        process_id: ProcessId,
        pid: u32,
    },
    ProcessCreateFailed {
        process_id: ProcessId,
        reason: String,
    },
    ProcessOutput {
        process_id: ProcessId,
        data: Vec<u8>,
    },
    ViewportPatch {
        process_id: ProcessId,
        changed_lines: Vec<(u32, TermLine)>,
        cursor: TermCursor,
        cols: u16,
        rows: u16,
        selection: Option<TermSelectionRange>,
        copy_mode: bool,
        full: bool,
        first_row: u32,
        total_rows: u32,
        alt: bool,
        mouse: bool,
        evicted_total: u64,
    },
    ProcessExited {
        process_id: ProcessId,
        exit_code: Option<i32>,
    },
    ProcessTitle {
        process_id: ProcessId,
        title: String,
    },
    CommandLifecycle {
        process_id: ProcessId,
        kind: CommandLifecycleKind,
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
        process_id: ProcessId,
        text: String,
    },
    TerminalMode {
        process_id: ProcessId,
        mouse_capture: bool,
        copy_mode: bool,
        alt_screen: bool,
        focus_reporting: bool,
    },
    AgentCommand {
        request_id: AgentRequestId,
        anchor: Option<ProcessId>,
        command: AgentCommand,
    },
    AgentQuery {
        request_id: AgentRequestId,
        query: AgentQuery,
    },
    AgentLayoutResult {
        request_id: AgentRequestId,
        result: Result<crate::protocol::layout::LayoutSnapshot, String>,
    },
    AgentTerminalReadResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentTerminalReadFullResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentCommandExitResult {
        request_id: AgentRequestId,
        result: Result<AgentCommandExit, String>,
    },
    AgentRunCompletionResult {
        request_id: AgentRequestId,
        result: Result<AgentRunCompletion, String>,
    },
    AgentSettingsResult {
        request_id: AgentRequestId,
        result: Result<JsonValue, String>,
    },
    AgentSpacesResult {
        request_id: AgentRequestId,
        result: Result<Vec<AgentSpace>, String>,
    },
    AgentScreenshotResult {
        request_id: AgentRequestId,
        result: Result<AgentImage, String>,
    },
    AgentBrowserSnapshotResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentBrowserScrollResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentRecordStartResult {
        request_id: AgentRequestId,
        result: Result<u32, String>,
    },
    AgentRecordStopResult {
        request_id: AgentRequestId,
        result: Result<AgentRecording, String>,
    },
    AgentBookmarksResult {
        request_id: AgentRequestId,
        result: Result<AgentBookmarks, String>,
    },
    AgentSimulatorScreenshotResult {
        request_id: AgentRequestId,
        result: Result<AgentImage, String>,
    },
    AgentSimulatorControlResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentWorkingDirectoryResult {
        request_id: AgentRequestId,
        result: Result<String, String>,
    },
    AgentVaultStatusResult {
        request_id: AgentRequestId,
        result: Result<crate::vault::VaultStatusSnapshot, String>,
    },
    AgentCommandsResult {
        request_id: AgentRequestId,
        result: Result<Vec<AgentCommandTool>, String>,
    },
    AgentCommandResult {
        request_id: AgentRequestId,
        result: AgentCommandResult,
    },
    Bell {
        process_id: ProcessId,
    },
    AgentToolCall {
        request_id: AgentRequestId,
        sid: String,
        name: String,
        args: JsonValue,
    },
    AcpTerminalCreated {
        sid: String,
        terminal_id: String,
        process_id: ProcessId,
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
    },
    AcpProposedDiff {
        sid: String,
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    StatusResponse {
        uptime_secs: u64,
        process_count: u32,
    },
    AcpSessionCreated {
        sid: String,
        acp_session_id: String,
    },
    AcpModelSelectionResult {
        sid: String,
        request_id: u64,
        model_id: String,
        succeeded: bool,
    },
    Shared(SharedEvent),
    AcpModeInfo {
        sid: String,
        config_id: String,
        current_mode_id: String,
        modes: Vec<AcpModeOption>,
    },
    AcpModeSelectionResult {
        sid: String,
        request_id: u64,
        mode_id: String,
        succeeded: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct AcpModelOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[vmux_api::contract(Eq)]
pub struct AcpModeOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}
