use crate::run_state::AgentRunState;
use crate::run_state_kind::{AgentRunStateKind, LastRunStateKind};
use bevy::prelude::*;
use bevy_cef::prelude::UiEventPlugin;
use vmux_session::{AcpSession, AgentSession};

pub(crate) struct ToastPlugin;

impl Plugin for ToastPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentToast>()
            .add_plugins(UiEventPlugin::<(AgentToast,)>::default())
            .add_systems(Update, surface_errors);
    }
}

#[vmux_api::contract(Copy, Eq)]
pub enum ToastLevel {
    Info,
    Warning,
    Error,
}

#[vmux_api::ui_event(Message, targets = ["agent", "agents"])]
pub struct AgentToast {
    pub session_sid: String,
    pub level: ToastLevel,
    pub message: String,
}

fn surface_errors(
    mut writer: MessageWriter<AgentToast>,
    mut sessions: Query<(
        &AgentRunState,
        &mut LastRunStateKind,
        Option<&AgentSession>,
        Option<&AcpSession>,
    )>,
) {
    for (state, mut last, page, acp) in &mut sessions {
        let Some(sid) = page
            .map(|session| session.sid.clone())
            .or_else(|| acp.map(|session| session.sid.clone()))
        else {
            continue;
        };
        let current = AgentRunStateKind::from(state);
        if last.0 == current {
            continue;
        }
        last.0 = current;
        if current != AgentRunStateKind::Errored {
            continue;
        }
        let AgentRunState::Errored(message) = state else {
            continue;
        };
        writer.write(AgentToast {
            session_sid: sid,
            level: ToastLevel::Error,
            message: message.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, AgentVariant};

    struct TestApp;

    impl TestApp {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(bevy::app::TaskPoolPlugin::default())
                .add_message::<AgentToast>()
                .add_systems(Update, surface_errors);
            app
        }

        fn session() -> AgentSession {
            AgentSession {
                kind: AgentKind::Vibe,
                variant: AgentVariant::Page,
                sid: "abc".into(),
                provider: "mock".into(),
                model: "m".into(),
            }
        }

        fn toasts(app: &mut App) -> Vec<AgentToast> {
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
                .drain()
                .collect()
        }
    }

    #[test]
    fn rkyv_roundtrip() {
        let t = AgentToast {
            session_sid: "abc".into(),
            level: ToastLevel::Error,
            message: "boom".into(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&t).expect("ser");
        let back: AgentToast =
            rkyv::from_bytes::<AgentToast, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(back.session_sid, "abc");
        assert_eq!(back.level, ToastLevel::Error);
        assert!(back.message.contains("boom"));
    }

    #[test]
    fn errored_transition_fires_toast() {
        let mut app = TestApp::app();
        app.world_mut().spawn((
            TestApp::session(),
            LastRunStateKind::default(),
            AgentRunState::Errored("boom".into()),
        ));
        app.update();
        let events = TestApp::toasts(&mut app);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_sid, "abc");
        assert_eq!(events[0].level, ToastLevel::Error);
        assert!(events[0].message.contains("boom"));
    }

    #[test]
    fn acp_errored_transition_fires_toast() {
        let mut app = TestApp::app();
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
        let events = TestApp::toasts(&mut app);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_sid, "acp1");
        assert!(events[0].message.contains("kaboom"));
    }

    #[test]
    fn no_op_when_state_kind_unchanged() {
        let mut app = TestApp::app();
        app.world_mut().spawn((
            TestApp::session(),
            LastRunStateKind(AgentRunStateKind::Errored),
            AgentRunState::Errored("old".into()),
        ));
        app.update();
        assert!(TestApp::toasts(&mut app).is_empty());
    }
}
