use bevy::prelude::*;
use bevy_cef::prelude::UiEventPlugin;
#[cfg(test)]
use vmux_session::Session;
use vmux_session::{RunState, SessionId};

pub(super) struct ToastPlugin;

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

#[vmux_api::ui_event(Message)]
pub struct AgentToast {
    pub session_sid: String,
    pub level: ToastLevel,
    pub message: String,
}

#[derive(Component)]
pub(super) struct ErrorSurfaced;

fn surface_errors(
    mut commands: Commands,
    mut writer: MessageWriter<AgentToast>,
    sessions: Query<
        (
            Entity,
            &RunState,
            Option<&vmux_session::RunError>,
            &SessionId,
            Option<&ErrorSurfaced>,
        ),
        Or<(Changed<RunState>, Changed<vmux_session::RunError>)>,
    >,
) {
    for (entity, state, error, session, surfaced) in &sessions {
        if !matches!(state, RunState::Errored) {
            if surfaced.is_some() {
                commands.entity(entity).remove::<ErrorSurfaced>();
            }
            continue;
        }
        let Some(error) = error else {
            continue;
        };
        if surfaced.is_some() {
            continue;
        }
        commands.entity(entity).insert(ErrorSurfaced);
        if error.in_use() {
            continue;
        }
        writer.write(AgentToast {
            session_sid: session.0.clone(),
            level: ToastLevel::Error,
            message: error.0.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_active_writer_refusal_does_not_emit_an_error_toast() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ToastPlugin));
        app.world_mut().spawn((
            Session,
            SessionId("busy".into()),
            RunState::Errored,
            vmux_session::RunError("thread_active_writer".into()),
        ));
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<AgentToast>>()
                .drain()
                .next()
                .is_none()
        );
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
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ToastPlugin));
        app.world_mut().spawn((
            Session,
            SessionId("abc".into()),
            RunState::Errored,
            vmux_session::RunError("boom".into()),
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
        app.add_plugins((MinimalPlugins, ToastPlugin));
        app.world_mut().spawn((
            Session,
            SessionId("acp1".into()),
            RunState::Errored,
            vmux_session::RunError("kaboom".into()),
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
        app.add_plugins((MinimalPlugins, ToastPlugin));
        app.world_mut().spawn((
            Session,
            SessionId("abc".into()),
            ErrorSurfaced,
            RunState::Errored,
            vmux_session::RunError("old".into()),
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
