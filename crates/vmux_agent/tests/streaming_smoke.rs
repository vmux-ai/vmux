use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy_cef::prelude::BinIpcEventRawBuffer;
use vmux_agent::{
    AgentApprovalPolicy, AgentKind, AgentMessages, AgentRunState, AgentSession, AgentVariant,
    AppAgentPlugin, AppAgentStrategy, AssistantBlock, LastRunStateKind, Message, PendingUserInput,
    providers::openai_shared::parse_chat_completions_sse,
    strategy::{AgentStrategies, AgentStrategy},
    stream::{StreamEvent, ToolDef},
};

struct MockMistral {
    url: String,
}

impl AgentStrategy for MockMistral {
    fn kind(&self) -> AgentKind {
        AgentKind::Vibe
    }
    fn variant(&self) -> AgentVariant {
        AgentVariant::App
    }
}

impl AppAgentStrategy for MockMistral {
    fn provider(&self) -> &str {
        "mistral"
    }
    fn model(&self) -> &str {
        "devstral-2"
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
        parse_chat_completions_sse(payload)
    }
}

#[test]
fn single_text_turn_streams_into_assistant_message() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let mut server = mockito::Server::new();
    let body = include_str!("fixtures/mistral/text.sse");
    let _m = server
        .mock("POST", "/chat")
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .create();

    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default());
    app.init_resource::<BinIpcEventRawBuffer>();
    app.add_plugins(AppAgentPlugin);
    let mut strategies = AgentStrategies::default();
    strategies.register_app(Arc::new(MockMistral {
        url: format!("{}/chat", server.url()),
    }));
    app.insert_resource(strategies);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
                variant: AgentVariant::App,
                sid: "smoke".into(),
                provider: "mistral".into(),
                model: "devstral-2".into(),
            },
            AgentMessages::default(),
            AgentApprovalPolicy::default(),
            AgentRunState::Idle,
            LastRunStateKind::default(),
            PendingUserInput("hi".into()),
        ))
        .id();

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.update();
        if matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Idle)
        ) {
            let msgs = app.world().get::<AgentMessages>(entity).unwrap();
            let last = msgs.0.last();
            if let Some(Message::Assistant { blocks }) = last {
                let text: String = blocks
                    .iter()
                    .filter_map(|b| match b {
                        AssistantBlock::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .collect();
                if text == "hello world" {
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("did not reach Idle with assistant 'hello world' within 5s");
}
