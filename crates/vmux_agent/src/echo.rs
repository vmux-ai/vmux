use crate::app::AppAgentStrategy;
use crate::message::Message;
use crate::strategy::AgentStrategy;
use crate::stream::{StopReason, StreamEvent, ToolDef};
use crate::{AgentKind, AgentVariant};

pub struct EchoAppStrategy;

impl AgentStrategy for EchoAppStrategy {
    fn kind(&self) -> AgentKind {
        AgentKind::Vibe
    }
    fn variant(&self) -> AgentVariant {
        AgentVariant::App
    }
}

impl AppAgentStrategy for EchoAppStrategy {
    fn provider(&self) -> &'static str {
        "stub"
    }
    fn model(&self) -> &'static str {
        "echo"
    }
    fn models(&self) -> &'static [&'static str] {
        &["echo"]
    }
    fn default_model(&self) -> &'static str {
        "echo"
    }
    fn endpoint(&self) -> &'static str {
        "stub://echo"
    }

    fn build_request(
        &self,
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

    fn parse_sse_event(&self, _payload: &str) -> Option<StreamEvent> {
        None
    }
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
