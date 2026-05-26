use std::sync::Mutex;
use std::time::Duration;

use bevy::prelude::*;
use bevy_cef::prelude::BinIpcEventRawBuffer;
use serial_test::serial;
use vmux_agent::client::page::strategy_components::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use vmux_agent::stream::ToolDef;
use vmux_agent::{
    AgentApprovalPolicy, AgentKind, AgentMessages, AgentRunState, AgentSession, AgentVariant,
    AssistantBlock, LastRunStateKind, Message, PageAgentPlugin, PendingUserInput,
};

static MOCK_URL: Mutex<Option<String>> = Mutex::new(None);

fn mock_build_request(_: &str, _: &[Message], _: &[ToolDef], _: &str) -> reqwest::Request {
    let url = MOCK_URL.lock().unwrap().clone().expect("MOCK_URL not set");
    reqwest::Client::new()
        .post(&url)
        .body("{}")
        .build()
        .unwrap()
}

#[test]
#[serial]
fn single_text_turn_streams_into_assistant_message() {
    unsafe { std::env::remove_var("MISTRAL_API_KEY") };

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

    *MOCK_URL.lock().unwrap() = Some(format!("{}/chat", server.url()));

    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .init_resource::<BinIpcEventRawBuffer>()
        .add_plugins(PageAgentPlugin);

    app.world_mut().spawn((
        Strategy,
        StrategyKey {
            provider: "mistral".into(),
            model: "devstral-2".into(),
        },
        Endpoint("http://mock/".into()),
        EnvVarName(""),
        StrategyKind(AgentKind::Vibe),
        StrategyVariant(AgentVariant::Page),
        BuildRequestFn(mock_build_request),
        ParseSseFn(vmux_agent::providers::mistral::parse_sse),
    ));
    app.update();

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
                variant: AgentVariant::Page,
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
                    *MOCK_URL.lock().unwrap() = None;
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    *MOCK_URL.lock().unwrap() = None;
    panic!("did not reach Idle with assistant 'hello world' within 5s");
}
