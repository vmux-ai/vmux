use bevy::prelude::*;
use vmux_agent::message::Message;
use vmux_agent::{
    AgentApprovalPolicy, AgentKind, AgentMessages, AgentRunState, AgentSession, AgentVariant,
    AssistantBlock, GuiAgentPlugin, PendingUserInput,
};

fn make_app() -> App {
    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default());
    app.add_plugins(GuiAgentPlugin);
    app
}

#[test]
fn echo_session_streams_to_assistant_message() {
    let mut app = make_app();
    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
                variant: AgentVariant::Gui,
                sid: "smoke".into(),
                provider: "vibe".into(),
                model: "echo-stub".into(),
            },
            AgentMessages::default(),
            AgentApprovalPolicy::default(),
            AgentRunState::Idle,
            PendingUserInput("hello".into()),
        ))
        .id();

    for _ in 0..50 {
        app.update();
        let state = app.world().get::<AgentRunState>(entity).unwrap();
        if matches!(state, AgentRunState::Idle) {
            let msgs = app.world().get::<AgentMessages>(entity).unwrap();
            if msgs.0.len() >= 2 {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
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
