use crate::message::Message;
#[cfg(test)]
use crate::stream::StopReason;
use crate::stream::{StreamEvent, ToolDef};

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

#[cfg(test)]
fn synthetic_echo_stream(text: &str) -> Vec<StreamEvent> {
    vec![
        StreamEvent::TextDelta(format!("echo: {text}")),
        StreamEvent::StopTurn {
            reason: StopReason::EndTurn,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_stream_returns_text_then_stop() {
        let events = synthetic_echo_stream("hi");
        assert_eq!(events.len(), 2);
        match &events[0] {
            StreamEvent::TextDelta(t) => assert_eq!(t, "echo: hi"),
            _ => panic!("expected text delta"),
        }
        assert!(matches!(events[1], StreamEvent::StopTurn { .. }));
    }
}
