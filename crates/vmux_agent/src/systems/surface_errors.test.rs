use super::*;
use crate::{AgentKind, AgentVariant};

fn make_app() -> App {
    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .add_message::<AgentToast>()
        .add_systems(Update, surface_errors);
    app
}

fn make_session() -> AgentSession {
    AgentSession {
        kind: AgentKind::Vibe,
        variant: AgentVariant::Page,
        sid: "abc".into(),
        provider: "mock".into(),
        model: "m".into(),
    }
}

#[test]
fn errored_transition_fires_toast() {
    let mut app = make_app();
    app.world_mut().spawn((
        make_session(),
        LastRunStateKind::default(),
        AgentRunState::Errored("boom".into()),
    ));
    app.update();
    let events: Vec<AgentToast> = app
        .world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
        .drain()
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].session_sid, "abc");
    assert_eq!(events[0].level, ToastLevel::Error);
    assert!(events[0].message.contains("boom"));
}

#[test]
fn acp_errored_transition_fires_toast() {
    use crate::client::acp::AcpSession;
    let mut app = make_app();
    app.world_mut().spawn((
        AcpSession {
            agent_id: "mistral-vibe".into(),
            sid: "acp1".into(),
            cwd: std::path::PathBuf::from("/tmp"),
            anchor: vmux_core::ProcessId::new(),
            resume: None,
        },
        LastRunStateKind::default(),
        AgentRunState::Errored("kaboom".into()),
    ));
    app.update();
    let events: Vec<AgentToast> = app
        .world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
        .drain()
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].session_sid, "acp1");
    assert!(events[0].message.contains("kaboom"));
}

#[test]
fn no_op_when_state_kind_unchanged() {
    let mut app = make_app();
    app.world_mut().spawn((
        make_session(),
        LastRunStateKind(AgentRunStateKind::Errored),
        AgentRunState::Errored("old".into()),
    ));
    app.update();
    let events: Vec<AgentToast> = app
        .world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
        .drain()
        .collect();
    assert!(events.is_empty());
}
