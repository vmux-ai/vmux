use bevy::prelude::*;
use bevy_cef::prelude::UiEventPlugin;
use vmux_session::{AcpSession, AgentRunState};

pub(super) fn add(app: &mut App) {
    app.add_message::<AgentToast>()
        .add_plugins(UiEventPlugin::<(AgentToast,)>::default())
        .add_systems(Update, surface_errors);
}

#[vmux_api::contract(Copy, Eq)]
pub enum ToastLevel {
    Info,
    Warning,
    Error,
}

#[vmux_api::ui_event(Message)]
pub struct AgentToast {
    pub session_sid: String,
    pub level: ToastLevel,
    pub message: String,
}

#[derive(Component)]
struct ErrorSurfaced;

fn surface_errors(
    mut commands: Commands,
    mut writer: MessageWriter<AgentToast>,
    sessions: Query<
        (Entity, &AgentRunState, &AcpSession, Option<&ErrorSurfaced>),
        Changed<AgentRunState>,
    >,
) {
    for (entity, state, session, surfaced) in &sessions {
        let AgentRunState::Errored(message) = state else {
            if surfaced.is_some() {
                commands.entity(entity).remove::<ErrorSurfaced>();
            }
            continue;
        };
        if surfaced.is_some() {
            continue;
        }
        commands.entity(entity).insert(ErrorSurfaced);
        writer.write(AgentToast {
            session_sid: session.sid.clone(),
            level: ToastLevel::Error,
            message: message.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::protocol::ProcessId;

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
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_message::<AgentToast>()
            .add_systems(Update, surface_errors);
        app.world_mut().spawn((
            AcpSession {
                agent_id: "mock".into(),
                sid: "abc".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: ProcessId::new(),
                resume: None,
            },
            AgentRunState::Errored("boom".into()),
        ));
        app.update();
        let events = app
            .world_mut()
            .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_sid, "abc");
        assert_eq!(events[0].level, ToastLevel::Error);
        assert!(events[0].message.contains("boom"));
    }

    #[test]
    fn acp_errored_transition_fires_toast() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_message::<AgentToast>()
            .add_systems(Update, surface_errors);
        app.world_mut().spawn((
            AcpSession {
                agent_id: "mistral-vibe".into(),
                sid: "acp1".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: vmux_ecs::ProcessId::new(),
                resume: None,
            },
            AgentRunState::Errored("kaboom".into()),
        ));
        app.update();
        let events = app
            .world_mut()
            .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_sid, "acp1");
        assert!(events[0].message.contains("kaboom"));
    }

    #[test]
    fn no_op_when_state_kind_unchanged() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_message::<AgentToast>()
            .add_systems(Update, surface_errors);
        app.world_mut().spawn((
            AcpSession {
                agent_id: "mock".into(),
                sid: "abc".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: ProcessId::new(),
                resume: None,
            },
            ErrorSurfaced,
            AgentRunState::Errored("old".into()),
        ));
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
                .drain()
                .next()
                .is_none()
        );
    }
}
