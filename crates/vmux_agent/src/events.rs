use bevy::prelude::*;
use serde_json::Value;
use std::collections::HashSet;

pub use vmux_service::agent_events::{
    AgentCommandRequest, AgentQueryRequest, AgentToolCallRequest, CommandOrigin,
};

#[derive(Event, Clone, Copy)]
pub struct WorkspacePickerStartRequest {
    pub webview: Entity,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    Allow,
    AllowAlways,
    Deny,
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
pub struct BrowserSnapshotRequest {
    pub request_id: [u8; 16],
    pub pane: Option<String>,
}

#[derive(Message, Clone)]
pub struct BrowserSnapshotResponse {
    pub request_id: [u8; 16],
    pub result: Result<String, String>,
}

#[derive(Message, Clone)]
pub struct BrowserScrollRequest {
    pub request_id: [u8; 16],
    pub pane: Option<String>,
    pub to: Option<String>,
    pub delta: Option<i32>,
}

/// Request ids whose `BrowserSnapshotResponse` must be returned as an agent
/// *command* result (a navigation that returns its page snapshot inline)
/// rather than the default *query* result. Populated when a deferred
/// navigation settles; drained by `forward_snapshot_responses`.
#[derive(Resource, Default)]
pub struct NavAwaitingSnapshot(pub HashSet<[u8; 16]>);

pub fn snapshot_response_to_query_result(
    result: &Result<String, String>,
) -> vmux_service::protocol::AgentQueryResult {
    use vmux_service::protocol::AgentQueryResult;
    match result {
        Ok(json) => AgentQueryResult::Text(json.clone()),
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::AgentQueryResult;

    #[test]
    fn ok_snapshot_maps_to_text() {
        let out = snapshot_response_to_query_result(&Ok("{\"url\":\"x\"}".to_string()));
        assert_eq!(out, AgentQueryResult::Text("{\"url\":\"x\"}".to_string()));
    }

    #[test]
    fn err_snapshot_maps_to_error() {
        let out = snapshot_response_to_query_result(&Err("no page".to_string()));
        assert_eq!(out, AgentQueryResult::Error("no page".to_string()));
    }
}
