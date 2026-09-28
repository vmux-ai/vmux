use bevy::prelude::*;
use serde_json::Value;
use vmux_api::ProcessId;
use vmux_api::protocol::{
    AcpModeOption, AcpModelOption, AgentRequest, AgentRequestId, AgentRunStatus, JsonValue,
};

pub use vmux_api::protocol::ApprovalDecision;
pub use vmux_core::agent::{AgentRequestInput, CommandOrigin};

#[vmux_core::service_message(AgentQuery)]
pub struct AgentQueryRequest {
    pub request_id: AgentRequestId,
    pub query: AgentRequest,
}

#[vmux_core::service_message(AgentToolCall)]
pub struct AgentToolCallRequest {
    pub request_id: AgentRequestId,
    pub sid: String,
    pub name: String,
    pub args: JsonValue,
}

#[vmux_core::service_message(Shared(SharedEvent::AgentDelta))]
pub struct UiAgentDelta {
    pub sid: String,
    pub text: String,
}

#[vmux_core::service_message(Shared(SharedEvent::AgentRunStatusChanged))]
pub struct UiAgentRunStatus {
    pub sid: String,
    pub status: AgentRunStatus,
}

#[derive(Message)]
pub struct UiAgentAwaitingApproval {
    pub sid: String,
    pub call_id: String,
    pub name: String,
    pub args: Value,
}

#[vmux_core::service_message(Shared(SharedEvent::AgentApprovalResolved))]
pub struct UiAgentApprovalResolved {
    pub sid: String,
    pub call_id: String,
}

#[vmux_core::service_message(Shared(SharedEvent::AgentMessagesSnapshot))]
pub struct UiAgentSnapshot {
    pub sid: String,
    pub messages: Vec<vmux_api::room::Message>,
}

#[vmux_core::service_message(Shared(SharedEvent::AcpAgentInfo))]
pub struct UiAgentInfo {
    pub sid: String,
    pub name: String,
}

#[vmux_core::service_message(Shared(SharedEvent::AcpWorkspaceChanged))]
pub struct UiAgentWorkspaceChanged {
    pub sid: String,
    pub name: String,
    pub branch: String,
    pub cwd: String,
    pub workspace_cwd: String,
}

#[vmux_core::service_message(Shared(SharedEvent::AcpModelInfo))]
pub struct UiAgentModelInfo {
    pub sid: String,
    pub config_id: String,
    pub current_model_id: String,
    pub models: Vec<AcpModelOption>,
}

#[vmux_core::service_message(AcpModelSelectionResult)]
pub struct UiAgentModelSelectionResult {
    pub sid: String,
    pub request_id: u64,
    pub model_id: String,
    pub succeeded: bool,
}

#[vmux_core::service_message(AcpModeInfo)]
pub struct UiAgentModeInfo {
    pub sid: String,
    pub config_id: String,
    pub current_mode_id: String,
    pub modes: Vec<AcpModeOption>,
}

#[vmux_core::service_message(AcpModeSelectionResult)]
pub struct UiAgentModeSelectionResult {
    pub sid: String,
    pub request_id: u64,
    pub mode_id: String,
    pub succeeded: bool,
}

#[vmux_core::service_message(AcpSessionCreated)]
pub struct UiAgentSessionCreated {
    pub sid: String,
    pub acp_session_id: String,
}

#[vmux_core::service_message(AcpTerminalCreated)]
pub struct UiAgentAcpTerminalCreated {
    pub sid: String,
    pub terminal_id: String,
    pub process_id: ProcessId,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

#[derive(Event, Clone, Copy)]
pub struct AgentChoiceSelected {
    pub webview: Entity,
    pub index: usize,
}

#[derive(Event, Clone, Debug)]
pub struct AgentInput {
    pub session: Entity,
    pub text: String,
}

#[derive(Event, Clone, Debug)]
pub struct AgentDelta {
    pub session: Entity,
    pub text: String,
}

#[derive(Event, Clone, Debug)]
pub struct AgentToolStatus {
    pub session: Entity,
    pub call_id: String,
    pub status: ToolStatus,
}

#[derive(Clone, Debug)]
pub enum ToolStatus {
    Pending,
    Running,
    Result { content: String, is_error: bool },
}

#[derive(Event, Clone, Debug)]
pub struct AgentApprovalRequest {
    pub session: Entity,
    pub call_id: String,
    pub name: String,
    pub args: Value,
}

#[derive(Event, Clone, Debug)]
pub struct AgentApprovalReply {
    pub session: Entity,
    pub call_id: String,
    pub decision: ApprovalDecision,
}

#[derive(Message, Clone)]
pub struct ScreenshotRequest {
    pub request_id: [u8; 16],
    pub pane: Option<String>,
}

#[derive(Clone)]
pub struct ScreenshotImage {
    pub path: String,
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Message, Clone)]
pub struct ScreenshotResponse {
    pub request_id: [u8; 16],
    pub result: Result<ScreenshotImage, String>,
}

#[derive(Message, Clone)]
pub struct RecordStartRequest {
    pub request_id: [u8; 16],
    pub gif: bool,
    pub max_secs: u32,
    pub pane: Option<String>,
}

#[derive(Message, Clone)]
pub struct RecordStartResponse {
    pub request_id: [u8; 16],
    pub result: Result<u32, String>,
}

#[derive(Message, Clone)]
pub struct RecordStopRequest {
    pub request_id: [u8; 16],
    pub dir: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone)]
pub struct RecordingInfo {
    pub mp4_path: String,
    pub gif_path: Option<String>,
    pub duration_ms: u64,
    pub bytes: u64,
    pub auto_stopped: bool,
}

#[derive(Message, Clone)]
pub struct RecordStopResponse {
    pub request_id: [u8; 16],
    pub result: Result<RecordingInfo, String>,
}
