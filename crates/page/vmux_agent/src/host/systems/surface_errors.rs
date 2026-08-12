use bevy::prelude::*;

use crate::run_state::AgentRunState;
use crate::run_state_kind::{AgentRunStateKind, LastRunStateKind};
use crate::toast::{AgentToast, ToastLevel};
use vmux_session::AcpSession;
use vmux_session::AgentSession;

/// On a transition into `Errored`, fire a toast. The chat page renders the errored run-state as a
/// styled inline card, so the error is not also pushed into the transcript (which duplicated it).
pub fn surface_errors(
    mut writer: MessageWriter<AgentToast>,
    mut q: Query<(
        &AgentRunState,
        &mut LastRunStateKind,
        Option<&AgentSession>,
        Option<&AcpSession>,
    )>,
) {
    for (state, mut last, page, acp) in &mut q {
        // Resolve the session id from either a Page/CLI session or an ACP session.
        let Some(sid) = page
            .map(|s| s.sid.clone())
            .or_else(|| acp.map(|s| s.sid.clone()))
        else {
            continue;
        };
        let cur = AgentRunStateKind::from(state);
        if last.0 == cur {
            continue;
        }
        last.0 = cur;
        if cur != AgentRunStateKind::Errored {
            continue;
        }
        let AgentRunState::Errored(msg) = state else {
            continue;
        };
        writer.write(AgentToast {
            session_sid: sid,
            level: ToastLevel::Error,
            message: msg.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
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
        use vmux_session::AcpSession;
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
}
