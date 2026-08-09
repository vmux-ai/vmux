use crate::message::Message;
use crate::stream::{StopReason, StreamEvent, ToolDef};

pub const PROVIDER: &str = "echo";
pub const ENDPOINT: &str = "stub://echo";
pub const ENV_VAR: &str = "";
pub const DEFAULT_MODEL: &str = "echo";

pub fn build_request(
    _model: &str,
    _messages: &[Message],
    _tools: &[ToolDef],
    _api_key: &str,
) -> reqwest::Request {
    reqwest::Client::new()
        .get("http://localhost/echo-stub-unused")
        .build()
        .unwrap()
}

pub fn parse_sse(_payload: &str) -> Option<StreamEvent> {
    None
}

pub fn synthetic_echo_stream(text: &str) -> Vec<StreamEvent> {
    vec![
        StreamEvent::TextDelta(format!("echo: {text}")),
        StreamEvent::StopTurn {
            reason: StopReason::EndTurn,
        },
    ]
}

#[cfg(test)]
#[path = "echo.test.rs"]
mod tests;
