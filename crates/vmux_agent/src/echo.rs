use crate::client::page::strategy::AgentPageStrategy;
use crate::message::Message;
use crate::strategy::AgentStrategy;
use crate::stream::{StopReason, StreamEvent, ToolDef};
use crate::{AgentKind, AgentVariant};

pub struct EchoPageStrategy {
    provider: String,
    model: String,
    kind: AgentKind,
}

impl EchoPageStrategy {
    pub fn new(provider: impl Into<String>, model: impl Into<String>, kind: AgentKind) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            kind,
        }
    }
}

impl AgentStrategy for EchoPageStrategy {
    fn kind(&self) -> AgentKind {
        self.kind
    }
    fn variant(&self) -> AgentVariant {
        AgentVariant::Page
    }
}

impl AgentPageStrategy for EchoPageStrategy {
    fn provider(&self) -> &str {
        &self.provider
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn endpoint(&self) -> &str {
        "stub://echo"
    }
    fn env_var(&self) -> &'static str {
        ""
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
        parse_sse(_payload)
    }

    fn parse_sse_fn(&self) -> crate::client::page::strategy_components::ParseSse {
        parse_sse
    }
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
