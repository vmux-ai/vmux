use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use crossbeam_channel::unbounded;

use crate::components::{AgentMessages, AgentSession, PendingUserInput};
use crate::echo::synthetic_echo_stream;
use crate::message::Message;
use crate::run_state::AgentRunState;
use crate::stream::StreamEvent;

pub fn process_user_input(
    mut commands: Commands,
    mut q: Query<
        (
            Entity,
            &PendingUserInput,
            &mut AgentMessages,
            &mut AgentRunState,
            &AgentSession,
        ),
        With<PendingUserInput>,
    >,
) {
    for (entity, pending, mut messages, mut state, _session) in &mut q {
        if !matches!(*state, AgentRunState::Idle) {
            continue;
        }
        messages.0.push(Message::User {
            text: pending.0.clone(),
        });

        let (tx, rx) = unbounded::<StreamEvent>();
        let text = pending.0.clone();
        let task = IoTaskPool::get().spawn(async move {
            for event in synthetic_echo_stream(&text) {
                let _ = tx.send(event);
            }
        });

        *state = AgentRunState::Streaming {
            rx,
            _task: task,
            partial: None,
        };
        commands.entity(entity).remove::<PendingUserInput>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, AgentVariant};

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_systems(Update, process_user_input);
        app
    }

    #[test]
    fn pending_input_transitions_to_streaming() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                    variant: AgentVariant::App,
                    sid: "test".into(),
                    provider: "vibe".into(),
                    model: "echo-stub".into(),
                },
                AgentMessages::default(),
                AgentRunState::Idle,
                PendingUserInput("hi".into()),
            ))
            .id();

        app.update();

        let world = app.world();
        let state = world.get::<AgentRunState>(entity).unwrap();
        assert!(matches!(state, AgentRunState::Streaming { .. }));
        assert!(world.get::<PendingUserInput>(entity).is_none());
        let msgs = world.get::<AgentMessages>(entity).unwrap();
        assert_eq!(msgs.0.len(), 1);
        match &msgs.0[0] {
            Message::User { text } => assert_eq!(text, "hi"),
            _ => panic!("expected user message"),
        }
    }
}
