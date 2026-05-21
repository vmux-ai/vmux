use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy_cef::prelude::BinIpcEventRawBuffer;
use vmux_agent::message::Message;
use vmux_agent::{
    AgentApprovalPolicy, AgentKind, AgentMessages, AgentPageStrategy, AgentRunState, AgentSession,
    AgentVariant, AssistantBlock, LastRunStateKind, PageAgentPlugin, PendingUserInput,
    strategy::{AgentStrategies, AgentStrategy},
    stream::{StopReason, StreamEvent, ToolDef},
};

struct EchoMock {
    url: String,
}

impl AgentStrategy for EchoMock {
    fn kind(&self) -> AgentKind {
        AgentKind::Vibe
    }
    fn variant(&self) -> AgentVariant {
        AgentVariant::Page
    }
}

impl AgentPageStrategy for EchoMock {
    fn provider(&self) -> &str {
        "vibe"
    }
    fn model(&self) -> &str {
        "echo-stub"
    }
    fn endpoint(&self) -> &str {
        &self.url
    }
    fn env_var(&self) -> &'static str {
        ""
    }
    fn build_request(&self, _: &str, _: &[Message], _: &[ToolDef], _: &str) -> reqwest::Request {
        reqwest::Client::new()
            .post(&self.url)
            .body("{}")
            .build()
            .unwrap()
    }
    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent> {
        echo_mock_parse_sse(payload)
    }
    fn parse_sse_fn(&self) -> vmux_agent::client::page::strategy_components::ParseSse {
        echo_mock_parse_sse
    }
}

fn echo_mock_parse_sse(payload: &str) -> Option<StreamEvent> {
    for line in payload.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[STOP]" {
                return Some(StreamEvent::StopTurn {
                    reason: StopReason::EndTurn,
                });
            }
            return Some(StreamEvent::TextDelta(data.to_string()));
        }
    }
    None
}

#[test]
fn echo_session_streams_to_assistant_message() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let mut server = mockito::Server::new();
    let body = "data: echo: hello\n\ndata: [STOP]\n\n";
    let _m = server
        .mock("POST", "/echo")
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .create();

    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default());
    app.init_resource::<BinIpcEventRawBuffer>();
    app.add_plugins(PageAgentPlugin);
    let mut strategies = AgentStrategies::default();
    strategies.register_page(Arc::new(EchoMock {
        url: format!("{}/echo", server.url()),
    }));
    app.insert_resource(strategies);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
                variant: AgentVariant::Page,
                sid: "smoke".into(),
                provider: "vibe".into(),
                model: "echo-stub".into(),
            },
            AgentMessages::default(),
            AgentApprovalPolicy::default(),
            AgentRunState::Idle,
            LastRunStateKind::default(),
            PendingUserInput("hello".into()),
        ))
        .id();

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.update();
        let state = app.world().get::<AgentRunState>(entity).unwrap();
        if matches!(state, AgentRunState::Idle) {
            let msgs = app.world().get::<AgentMessages>(entity).unwrap();
            if msgs.0.len() >= 2 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let msgs = app.world().get::<AgentMessages>(entity).unwrap();
    assert_eq!(msgs.0.len(), 2, "expected user + assistant messages");
    assert!(matches!(&msgs.0[0], Message::User { text } if text == "hello"));
    let assistant_text = match &msgs.0[1] {
        Message::Assistant { blocks } => blocks
            .iter()
            .filter_map(|b| match b {
                AssistantBlock::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<String>(),
        _ => panic!("expected assistant message"),
    };
    assert_eq!(assistant_text, "echo: hello");
}
